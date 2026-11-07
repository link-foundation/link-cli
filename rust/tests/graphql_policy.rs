//! The GraphQL executor under a permission policy, run inside this test binary.
//! The C# port has the same cases in `GraphqlPolicyTests.cs`.

use std::path::Path;
use std::process::Command;

use link_cli::graphql::{execute, Auth, DeepStore, Json, Settings};
use link_cli::{NamedTypesDecorator, PermissionPolicy};

/// The markers of the permission layer and `rule`, written without a policy.
fn write_rules(db: &Path, rule: &str) {
    let query = format!(
        "() ((Type: Type Type) (Extends: Extends Extends) (AllowRead: AllowRead AllowRead) (AllowWrite: AllowWrite AllowWrite) (Anything: Anything Anything) (Doc: Type Doc) {rule})"
    );
    let written = Command::new(env!("CARGO_BIN_EXE_clink"))
        .arg("--db")
        .arg(db)
        .arg("--auto-create-missing-references")
        .arg(query)
        .output()
        .unwrap();
    assert!(written.status.success(), "{written:?}");
}

/// Runs `body` over a store under the policy of `rule`, with Deep's core loaded.
fn with_store(rule: &str, body: impl FnOnce(&mut DeepStore<'_>, &Settings)) {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let db = dir.path().join("db.links");
    write_rules(&db, rule);
    let mut links = NamedTypesDecorator::new(&db, false)
        .unwrap()
        .with_indexed_lookups();
    let policy = PermissionPolicy::in_data(&mut links).unwrap();
    let mut store = DeepStore::open(&mut links, Some(policy)).unwrap();
    let core = Json::parse(include_str!("../../docs/protocol/graphql/deep-core.json")).unwrap();
    store.load_package(&core).unwrap();
    body(&mut store, &Settings::new(b"secret".to_vec(), None));
}

fn ask(store: &mut DeepStore<'_>, settings: &Settings, query: &str, auth: Auth) -> String {
    execute(store, settings, query, None, &Json::Null, auth)
        .0
        .write()
}

fn user(id: i64) -> Auth {
    Auth {
        user: Some(id),
        admin: false,
    }
}

const ADMIN: Auth = Auth {
    user: None,
    admin: true,
};

fn guest(store: &mut DeepStore<'_>, settings: &Settings) -> i64 {
    let answer = execute(
        store,
        settings,
        "{guest{linkId}}",
        None,
        &Json::Null,
        Auth::default(),
    )
    .0;
    answer
        .get("data")
        .and_then(|data| data.get("guest"))
        .and_then(|guest| guest.get("linkId"))
        .and_then(Json::as_i64)
        .unwrap()
}

#[test]
fn what_a_subject_created_is_only_its_own() {
    with_store("(AllowWrite (Anything Doc))", |store, settings| {
        let (first, second) = (guest(store, settings), guest(store, settings));
        assert_ne!(first, second);
        let created = ask(
            store,
            settings,
            r#"mutation{insert_links_one(object:{id:9000,type_id:9000,string:{data:{value:"mine"}}}){id}}"#,
            user(first),
        );
        assert_eq!(created, r#"{"data":{"insert_links_one":{"id":9000}}}"#);

        let links = "{links(where:{id:{_eq:9000}}){id string{value}}}";
        let seen = r#"{"data":{"links":[{"id":9000,"string":{"value":"mine"}}]}}"#;
        let unseen = r#"{"data":{"links":[]}}"#;
        assert_eq!(ask(store, settings, links, user(first)), seen);
        assert_eq!(ask(store, settings, links, user(second)), unseen);
        assert_eq!(ask(store, settings, links, Auth::default()), unseen);
        assert_eq!(ask(store, settings, links, ADMIN), seen);

        // A value is seen with its owner.
        let values = r#"{strings(where:{value:{_eq:"mine"}}){value}}"#;
        let value = r#"{"data":{"strings":[{"value":"mine"}]}}"#;
        assert_eq!(ask(store, settings, values, user(first)), value);
        assert_eq!(
            ask(store, settings, values, user(second)),
            r#"{"data":{"strings":[]}}"#
        );
    });
}

#[test]
fn writing_what_is_not_yours_is_forbidden() {
    with_store("(AllowRead (Anything Anything))", |store, settings| {
        let owner = ADMIN;
        ask(
            store,
            settings,
            r#"mutation{insert_links_one(object:{id:9000,type_id:1,number:{data:{value:1}}}){id}}"#,
            owner,
        );
        let visitor = guest(store, settings);
        let forbidden = r#""code":"permission-error""#;
        for mutation in [
            "mutation{insert_links_one(object:{type_id:9000}){id}}",
            "mutation{update_links_by_pk(id:9000,_set:{to_id:1}){id}}",
            "mutation{delete_links_by_pk(id:9000){id}}",
            "mutation{update_numbers(where:{link_id:{_eq:9000}},_set:{value:2}){affected_rows}}",
            "mutation{delete_numbers(where:{link_id:{_eq:9000}}){affected_rows}}",
            r#"mutation{insert_strings_one(object:{link_id:9000,value:"x"}){id}}"#,
        ] {
            let answer = ask(store, settings, mutation, user(visitor));
            assert!(answer.contains(forbidden), "{mutation}: {answer}");
        }
        // Nothing changed, and nothing was left half done.
        assert_eq!(
            ask(
                store,
                settings,
                "{links(where:{id:{_eq:9000}}){to_id number{value}}}",
                user(visitor)
            ),
            r#"{"data":{"links":[{"to_id":0,"number":{"value":1}}]}}"#
        );
    });
}

#[test]
fn a_token_for_another_link_needs_permission() {
    with_store("(AllowRead (Anything Anything))", |store, settings| {
        let (first, second) = (guest(store, settings), guest(store, settings));
        let of = |id: i64| format!("{{jwt(input:{{linkId:{id}}}){{linkId error}}}}");
        assert_eq!(
            ask(store, settings, &of(second), user(first)),
            r#"{"data":{"jwt":{"linkId":null,"error":"Forbidden."}}}"#
        );
        assert_eq!(
            ask(store, settings, &of(second), Auth::default()),
            r#"{"data":{"jwt":{"linkId":null,"error":"Forbidden."}}}"#
        );
        let own = format!(r#"{{"data":{{"jwt":{{"linkId":{first},"error":null}}}}}}"#);
        assert_eq!(ask(store, settings, &of(first), user(first)), own);
        assert_eq!(ask(store, settings, &of(first), ADMIN), own);
        // No link names the asker.
        assert_eq!(
            ask(store, settings, "{jwt{linkId error}}", user(first)),
            own
        );
    });
}

#[test]
fn can_lists_the_grants_in_deep_terms() {
    with_store("(AllowRead (Anything Anything))", |store, settings| {
        let answer = ask(
            store,
            settings,
            "{can{rule_id action_id object_id subject_id action{id}}}",
            ADMIN,
        );
        // AllowSelect of the loaded core, and 0 for `Anything` on both sides.
        assert_eq!(
            answer,
            r#"{"data":{"can":[{"rule_id":0,"action_id":28,"object_id":0,"subject_id":0,"action":{"id":28}}]}}"#
        );
        assert_eq!(
            ask(
                store,
                settings,
                "{links(where:{id:{_eq:28}}){can_action{subject_id} can_object{rule_id} can_rule{rule_id} can_subject{rule_id}}}",
                ADMIN
            ),
            r#"{"data":{"links":[{"can_action":[{"subject_id":0}],"can_object":[],"can_rule":[],"can_subject":[]}]}}"#
        );
    });
}

#[test]
fn authorization_reads_the_secret_and_the_token() {
    let settings = Settings::new(b"secret".to_vec(), Some("root".to_string()));
    let open = Settings::new(b"secret".to_vec(), None);
    let token = link_cli::graphql::crypto::sign_token(b"secret", 7, 0);
    let bearer = format!("Bearer {token}");

    assert_eq!(settings.authorize(None, None), Ok(Auth::default()));
    assert_eq!(settings.authorize(Some("  "), None), Ok(Auth::default()));
    assert_eq!(
        settings.authorize(Some("Bearer "), None),
        Ok(Auth::default())
    );
    assert_eq!(settings.authorize(Some(&bearer), None), Ok(user(7)));
    assert_eq!(settings.authorize(Some(&token), None), Ok(user(7)));
    assert_eq!(
        settings.authorize(Some(&bearer), Some("root")),
        Ok(Auth {
            user: Some(7),
            admin: true
        })
    );
    for answer in [
        settings.authorize(None, Some("wrong")),
        open.authorize(None, Some("root")),
    ] {
        assert_eq!(answer, Err("invalid x-hasura-admin-secret".to_string()));
    }
    let forged = settings.authorize(Some("a.b.c"), None).unwrap_err();
    assert!(forged.starts_with("Could not verify JWT"), "{forged}");

    let (a, b) = (Settings::random_secret(), Settings::random_secret());
    assert_eq!(a.len(), 32);
    assert_ne!(a, b);
}
