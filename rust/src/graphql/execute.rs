//! Runs a GraphQL document: authorisation, the root fields and introspection.

use std::collections::HashMap;
use std::rc::Rc;

use super::crypto::{sign_token, verify_token};
use super::document::{Document, Field, OperationKind, Selection};
use super::json::Json;
use super::store::{DeepStore, ValueKind};

/// An error as Hasura reports it.
#[derive(Debug, Clone)]
pub struct GqlError {
    pub message: String,
    pub code: &'static str,
    pub path: String,
}

impl GqlError {
    pub fn validation(message: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code: "validation-failed",
            path: path.into(),
        }
    }

    pub fn write(message: impl std::fmt::Display) -> Self {
        let message = message.to_string();
        let code = if message.contains("Forbidden") {
            "permission-error"
        } else if message.contains("violation") {
            "constraint-violation"
        } else {
            "unexpected"
        };
        Self {
            message,
            code,
            path: "$".to_string(),
        }
    }

    pub fn to_json(&self) -> Json {
        Json::object(vec![(
            "errors",
            Json::Array(vec![Json::object(vec![
                ("message", Json::str(&*self.message)),
                (
                    "extensions",
                    Json::object(vec![
                        ("path", Json::str(&*self.path)),
                        ("code", Json::str(self.code)),
                    ]),
                ),
            ])]),
        )])
    }
}

pub type GqlResult<T> = Result<T, GqlError>;

/// What the transport learned of who is asking.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Auth {
    /// The Deep id of the subject; `None` is a guest.
    pub user: Option<i64>,
    pub admin: bool,
}

/// What a server keeps for all its requests.
pub struct Settings {
    pub secret: Vec<u8>,
    pub admin_secret: Option<String>,
    introspection: Json,
}

impl Settings {
    pub fn new(secret: Vec<u8>, admin_secret: Option<String>) -> Self {
        let introspection = Json::parse(include_str!("introspection.json"))
            .ok()
            .and_then(|json| json.get("data").cloned())
            .unwrap_or(Json::Null);
        Self {
            secret,
            admin_secret,
            introspection,
        }
    }

    /// A secret for tokens that need not outlive this process.
    pub fn random_secret() -> Vec<u8> {
        use std::io::Read;
        let mut bytes = vec![0; 32];
        std::fs::File::open("/dev/urandom")
            .and_then(|mut random| random.read_exact(&mut bytes))
            .map(|()| bytes)
            .unwrap_or_else(|_| {
                use std::hash::{BuildHasher, Hasher};
                (0..4)
                    .flat_map(|_| {
                        let hasher = std::collections::hash_map::RandomState::new().build_hasher();
                        hasher.finish().to_le_bytes()
                    })
                    .collect()
            })
    }

    /// The subject of `Authorization: Bearer <jwt>` and of the admin secret.
    pub fn authorize(
        &self,
        authorization: Option<&str>,
        admin_secret: Option<&str>,
    ) -> Result<Auth, String> {
        let admin = match (&self.admin_secret, admin_secret) {
            (Some(expected), Some(given)) if expected == given => true,
            (_, Some(_)) => return Err("invalid x-hasura-admin-secret".to_string()),
            _ => false,
        };
        let token = authorization
            .map(|value| value.strip_prefix("Bearer ").unwrap_or(value).trim())
            .filter(|token| !token.is_empty());
        let user = match token {
            None => None,
            Some(token) => Some(
                verify_token(&self.secret, token, now())
                    .map(|id| id as i64)
                    .map_err(|error| format!("Could not verify JWT: {error}"))?,
            ),
        };
        Ok(Auth { user, admin })
    }
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

/// One request's run over the store.
pub struct Exec<'a, 's> {
    pub store: &'a mut DeepStore<'s>,
    pub settings: &'a Settings,
    pub document: &'a Document,
    pub variables: &'a Json,
    pub auth: Auth,
    /// The subject's address in the store; 0 is a guest.
    pub subject: u32,
    /// The remote columns of a relation's filter, kept for one root field.
    pub memo: HashMap<String, Rc<std::collections::HashSet<String>>>,
}

/// Runs an operation and returns the whole response.
pub fn execute(
    store: &mut DeepStore,
    settings: &Settings,
    query: &str,
    operation: Option<&str>,
    variables: &Json,
    auth: Auth,
) -> (Json, bool) {
    let document = match Document::parse(query) {
        Ok(document) => document,
        Err(error) => {
            return (
                GqlError::validation(error.to_string(), "$.query").to_json(),
                false,
            );
        }
    };
    let selected = match document.operation(operation) {
        Ok(selected) => selected,
        Err(error) => {
            return (
                GqlError::validation(error.to_string(), "$.query").to_json(),
                false,
            );
        }
    };
    // A variable a request leaves out takes the operation's default.
    let mut defaults: Vec<(String, Json)> = match variables {
        Json::Object(pairs) => pairs.clone(),
        _ => Vec::new(),
    };
    for (name, default) in &selected.variables {
        if let (Some(default), None) = (default, variables.get(name)) {
            defaults.push((name.clone(), default.resolve(&Json::Null)));
        }
    }
    let variables = Json::Object(defaults);
    store.admin = auth.admin;
    let subject = auth.user.and_then(|id| store.triplet_of(id)).unwrap_or(0);
    let mut exec = Exec {
        store,
        settings,
        document: &document,
        variables: &variables,
        auth,
        subject,
        memo: HashMap::new(),
    };
    let mutation = selected.kind == OperationKind::Mutation;
    if mutation {
        exec.store.begin();
    }
    let result = exec.root(&selected.selections, mutation);
    let response = match result {
        Ok(data) => {
            if mutation {
                if let Err(error) = exec.store.commit() {
                    return (GqlError::write(error).to_json(), false);
                }
            }
            (Json::object(vec![("data", data)]), mutation)
        }
        Err(error) => {
            if mutation {
                if let Err(undo) = exec.store.rollback() {
                    return (GqlError::write(undo).to_json(), false);
                }
            }
            (error.to_json(), false)
        }
    };
    exec.store.admin = false;
    response
}

impl<'a> Exec<'a, '_> {
    /// The fields of `selections` that apply to `typename`, without fragments.
    pub fn flatten(
        &self,
        selections: &'a [Selection],
        typename: &str,
    ) -> GqlResult<Vec<&'a Field>> {
        let mut fields = Vec::new();
        self.collect(selections, typename, &mut fields, 0)?;
        Ok(fields)
    }

    fn included(&self, directives: &[super::document::Directive]) -> bool {
        directives.iter().all(|directive| {
            let condition = directive
                .arguments
                .iter()
                .find(|(name, _)| name == "if")
                .map(|(_, value)| value.resolve(self.variables));
            !matches!(
                (directive.name.as_str(), condition),
                ("skip", Some(Json::Bool(true))) | ("include", Some(Json::Bool(false)))
            )
        })
    }

    fn collect(
        &self,
        selections: &'a [Selection],
        typename: &str,
        out: &mut Vec<&'a Field>,
        depth: usize,
    ) -> GqlResult<()> {
        if depth > 64 {
            return Err(GqlError::validation(
                "fragments are nested too deeply",
                "$.query",
            ));
        }
        for selection in selections {
            match selection {
                Selection::Field(field) => {
                    if self.included(&field.directives)
                        && !out
                            .iter()
                            .any(|seen| seen.response_key() == field.response_key())
                    {
                        out.push(field);
                    }
                }
                Selection::Spread(name, directives) => {
                    if self.included(directives) {
                        let Some(fragment) = self.document.fragments.get(name) else {
                            return Err(GqlError::validation(
                                format!("fragment '{name}' is undefined"),
                                "$.query",
                            ));
                        };
                        self.collect(fragment, typename, out, depth + 1)?;
                    }
                }
                Selection::Inline(on, directives, inner) => {
                    let applies = on.as_deref().is_none_or(|on| on == typename);
                    if applies && self.included(directives) {
                        self.collect(inner, typename, out, depth + 1)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn root(&mut self, selections: &'a [Selection], mutation: bool) -> GqlResult<Json> {
        let typename = if mutation {
            "mutation_root"
        } else {
            "query_root"
        };
        let mut data = Vec::new();
        for field in self.flatten(selections, typename)? {
            self.memo.clear();
            let value = self.root_field(field, typename, mutation)?;
            data.push((field.response_key().to_string(), value));
        }
        Ok(Json::Object(data))
    }

    fn root_field(&mut self, field: &'a Field, root: &str, mutation: bool) -> GqlResult<Json> {
        let name = field.name.as_str();
        if name == "__typename" {
            return Ok(Json::str(root));
        }
        if mutation {
            if let Some(found) = self.mutation_field(field)? {
                return Ok(found);
            }
        } else {
            match name {
                "__schema" => {
                    let schema = self
                        .settings
                        .introspection
                        .get("__schema")
                        .cloned()
                        .unwrap_or(Json::Null);
                    return self.project_schema(&schema, &field.selections, "__Schema");
                }
                "__type" => {
                    let wanted = field.argument("name", self.variables);
                    let found = self
                        .settings
                        .introspection
                        .get("__schema")
                        .and_then(|schema| schema.get("types"))
                        .map(Json::as_list)
                        .unwrap_or_default()
                        .into_iter()
                        .find(|item| {
                            item.get("name").and_then(Json::as_str)
                                == wanted.as_ref().and_then(Json::as_str)
                        })
                        .cloned()
                        .unwrap_or(Json::Null);
                    return self.project_schema(&found, &field.selections, "__Type");
                }
                "guest" => return self.guest(field),
                "jwt" => return self.jwt(field),
                _ => {}
            }
            if let Some(found) = self.select_field(field)? {
                return Ok(found);
            }
        }
        Err(GqlError::validation(
            format!("field '{name}' not found in type: '{root}'"),
            format!("$.selectionSet.{name}"),
        ))
    }

    /// Selects from the recorded schema: its keys are the field names.
    fn project_schema(
        &self,
        value: &Json,
        selections: &'a [Selection],
        typename: &str,
    ) -> GqlResult<Json> {
        match value {
            Json::Array(items) => Ok(Json::Array(
                items
                    .iter()
                    .map(|item| self.project_schema(item, selections, typename))
                    .collect::<GqlResult<_>>()?,
            )),
            Json::Object(_) => {
                let mut out = Vec::new();
                for field in self.flatten(selections, typename)? {
                    let name = field.name.as_str();
                    let found = if name == "__typename" {
                        Json::str(typename)
                    } else {
                        let inner = value.get(name).cloned().unwrap_or(Json::Null);
                        let next = match name {
                            "queryType" | "mutationType" | "subscriptionType" | "types"
                            | "type" | "ofType" | "interfaces" | "possibleTypes" => "__Type",
                            "fields" => "__Field",
                            "args" | "inputFields" => "__InputValue",
                            "enumValues" => "__EnumValue",
                            "directives" => "__Directive",
                            _ => typename,
                        };
                        if field.selections.is_empty() {
                            inner
                        } else {
                            self.project_schema(&inner, &field.selections, next)?
                        }
                    };
                    out.push((field.response_key().to_string(), found));
                }
                Ok(Json::Object(out))
            }
            other => Ok(other.clone()),
        }
    }

    fn token_response(
        &mut self,
        field: &'a Field,
        typename: &str,
        id: Option<i64>,
        error: Option<String>,
    ) -> GqlResult<Json> {
        let token = id.map(|id| sign_token(&self.settings.secret, id as u64, now()));
        let mut out = Vec::new();
        for item in self.flatten(&field.selections, typename)? {
            let value = match item.name.as_str() {
                "__typename" => Json::str(typename),
                "token" => token.clone().map_or(Json::Null, Json::String),
                "linkId" => id.map_or(Json::Null, Json::Int),
                "error" => error.clone().map_or(Json::Null, Json::String),
                other => {
                    return Err(GqlError::validation(
                        format!("field '{other}' not found in type: '{typename}'"),
                        format!("$.selectionSet.{}.{other}", field.name),
                    ))
                }
            };
            out.push((item.response_key().to_string(), value));
        }
        Ok(Json::Object(out))
    }

    /// `guest`: a new link of type `User`, with a token for it. Who asks does
    /// not matter, so the layer below is asked as the server.
    fn guest(&mut self, field: &'a Field) -> GqlResult<Json> {
        self.store.begin();
        let admin = std::mem::replace(&mut self.store.admin, true);
        let created = self.create_guest();
        self.store.admin = admin;
        match created {
            Ok(id) => {
                self.store.commit().map_err(GqlError::write)?;
                self.token_response(field, "GuestOutput", Some(id), None)
            }
            Err(error) => {
                self.store.rollback().map_err(GqlError::write)?;
                Err(GqlError::write(error))
            }
        }
    }

    fn create_guest(&mut self) -> anyhow::Result<i64> {
        let user = self.core_id("User");
        let id = self.store.reserve(1)[0];
        self.store
            .create_link(0, Some(id), user.unwrap_or(id), 0, 0)?;
        // A guest of a database with `deep/users` is one of the users.
        if let (Some(join), Some(users)) =
            (self.core_id("Join"), self.contained_in("deep", "users"))
        {
            self.store.create_link(0, None, join, users, id)?;
        }
        Ok(id)
    }

    /// `jwt(input: {linkId})`: a token for the asker or, with permission, for
    /// the link named; no link names the asker, as a "who am I" query.
    fn jwt(&mut self, field: &'a Field) -> GqlResult<Json> {
        let target = field
            .argument("input", self.variables)
            .and_then(|input| input.get("linkId").and_then(Json::as_i64));
        let asker = self.auth.user;
        let Some(target) = target.or(asker) else {
            return self.token_response(
                field,
                "JWTOutput",
                None,
                Some("No link and no subject.".to_string()),
            );
        };
        let permitted = self.auth.admin || !self.store.has_policy() || asker == Some(target);
        if !permitted {
            return self.token_response(field, "JWTOutput", None, Some("Forbidden.".to_string()));
        }
        if self.store.triplet_of(target).is_none() {
            return self.token_response(
                field,
                "JWTOutput",
                None,
                Some(format!("No link {target}.")),
            );
        }
        self.token_response(field, "JWTOutput", Some(target), None)
    }

    /// The link a `Contain` named `name` holds from the package named `package`.
    pub fn contained_in(&mut self, package: &str, name: &str) -> Option<i64> {
        let contains: Vec<i64> = self
            .store
            .values()
            .filter(|(_, row)| row.kind == ValueKind::String && row.value.as_str() == Some(name))
            .map(|(_, row)| row.owner)
            .collect();
        contains.into_iter().find_map(|contain| {
            let triplet = self.store.triplet_of(contain)?;
            let row = self.raw_row(triplet)?;
            let (_, parent) = self.store.value_of_owner(ValueKind::String, row.from_id)?;
            (parent.value.as_str() == Some(package)).then_some(row.to_id)
        })
    }

    /// The id of a type of `@deep-foundation/core`.
    pub fn core_id(&mut self, name: &str) -> Option<i64> {
        self.contained_in("@deep-foundation/core", name)
    }
}
