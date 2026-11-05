//! Sessions of a links server, each with its subject (issue #149).
//!
//! [`LinksSessions`] is what a [`LinksServer`](super::LinksServer) gives
//! every connection, without the network. The C# counterparts are
//! `LinksSessions` and `LinksSession`.

use std::collections::{HashMap, HashSet};
use std::panic::{self, AssertUnwindSafe};

use anyhow::Result;
use links_notation::binary::{format_document, parse_document, LinoDocument};
use links_notation::LiNo;

use super::links_operations::{Change, LinksOperation};
use super::server::{error_document, error_message, execute_observed_request, execute_request};
use super::triplets_operations::{execute_observed_triplets_request, TripletsOperation};
use crate::changes_simplifier::simplify_changes;
use crate::link::Link;
use crate::named_type_links::NamedTypeLinks;
use crate::permissions::{AccessPolicy, PermissionPolicy, PermissionsDecorator};
use crate::query_processor::QueryProcessor;
use crate::triplets::{TripletChange, TripletMarkers, TripletsOverDoublets};
use crate::undoable_links::UndoableLinksDecorator;

/// The subject of a token, or `None` when the token is not valid: answers
/// `(auth: token)`.
///
/// It is given the store the sessions serve, below any permissions, so a
/// [`LinksTokenTable`](crate::LinksTokenTable) kept in the data can read it:
/// `Box::new(|store, token| LinksTokenTable::new(store).authenticate(token))`.
/// An error fails the authentication.
pub type Authenticator = Box<dyn FnMut(&mut dyn NamedTypeLinks, &str) -> Result<Option<u32>>>;

/// A session of a [`LinksSessions`], as [`LinksSessions::open`] returns it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SessionId(u64);

/// What a session keeps between its requests.
#[derive(Debug, Default)]
struct Session {
    subject: u32,
    /// The links the subject created, while they are its own.
    created: HashSet<u32>,
}

enum Store<S: NamedTypeLinks, P: AccessPolicy> {
    Plain(S),
    Permitted(PermissionsDecorator<UndoableLinksDecorator<S>, P>),
}

/// A store served to many sessions, each with its subject (issue #149).
///
/// Without permissions every session sees and changes the whole store,
/// exactly as before permissions existed. With them
/// ([`with_permissions`](Self::with_permissions)), a session sees the store
/// through a [`PermissionsDecorator`] of its subject, and every request is
/// atomic: one that fails is undone whole, the writes the store made on its
/// own (cascades, merges) and the names it took included, even where they
/// touched links the subject cannot see.
///
/// `(auth: token)` sets the subject of the session to the one the
/// [`Authenticator`] gives the token, and answers `(subject: i)`; an unknown
/// token, or sessions without an authenticator, answer
/// `(error: 'Forbidden: authentication failed')`.
///
/// With [`with_triplets`](Self::with_triplets) the sessions read every request
/// as triplets (issue #152; see [`TripletsOperation`]), and permissions judge
/// each triplet as a whole, not the wrappers and pairs that store it.
///
/// Requests run one at a time, as `&mut self` already demands.
pub struct LinksSessions<S: NamedTypeLinks, P: AccessPolicy = PermissionPolicy> {
    store: Store<S, P>,
    processor: QueryProcessor,
    triplets: Option<TripletMarkers>,
    authenticate: Option<Authenticator>,
    subject: u32,
    sessions: HashMap<SessionId, Session>,
    next: u64,
}

impl<S: NamedTypeLinks> LinksSessions<S> {
    /// Sessions that see and change the whole store.
    pub fn new(store: S) -> Self {
        Self::of(Store::Plain(store))
    }
}

impl<S: NamedTypeLinks, P: AccessPolicy> LinksSessions<S, P> {
    /// Sessions that see the store as `policy` lets their subjects.
    pub fn with_permissions(store: S, policy: P) -> Self {
        Self::of(Store::Permitted(PermissionsDecorator::new(
            UndoableLinksDecorator::new(store),
            policy,
            0,
        )))
    }

    fn of(store: Store<S, P>) -> Self {
        Self {
            store,
            processor: QueryProcessor::new(false),
            triplets: None,
            authenticate: None,
            subject: 0,
            sessions: HashMap::new(),
            next: 0,
        }
    }

    /// Answers `(auth: token)` with `authenticate`.
    pub fn with_authenticator(
        mut self,
        authenticate: impl FnMut(&mut dyn NamedTypeLinks, &str) -> Result<Option<u32>> + 'static,
    ) -> Self {
        self.authenticate = Some(Box::new(authenticate));
        self
    }

    /// The subject of a session before any `(auth: token)`; the null link
    /// (0, the default) is a guest.
    pub fn with_subject(mut self, subject: u32) -> Self {
        self.subject = subject;
        self
    }

    /// Passed to [`QueryProcessor::with_auto_create_missing_references`].
    pub fn with_auto_create_missing_references(mut self, enabled: bool) -> Self {
        self.processor = QueryProcessor::new(false).with_auto_create_missing_references(enabled);
        self
    }

    /// Reads every request as triplets; the markers of the encoding are
    /// created in the store when they are missing, whatever the subject of a
    /// session may do.
    pub fn with_triplets(mut self) -> Result<Self> {
        self.ensure_triplets()?;
        Ok(self)
    }

    /// Like [`with_triplets`](Self::with_triplets), on sessions already built.
    pub fn ensure_triplets(&mut self) -> Result<TripletMarkers> {
        if let Some(markers) = self.triplets {
            return Ok(markers);
        }
        let markers = TripletMarkers::ensure(self.store_mut())?;
        self.triplets = Some(markers);
        Ok(markers)
    }

    /// Whether the sessions read every request as triplets.
    pub fn has_triplets(&self) -> bool {
        self.triplets.is_some()
    }

    /// Whether the sessions see the store through permissions.
    pub fn has_permissions(&self) -> bool {
        matches!(self.store, Store::Permitted(_))
    }

    /// The store.
    pub fn store(&self) -> &S {
        match &self.store {
            Store::Plain(store) => store,
            Store::Permitted(view) => view.inner().inner(),
        }
    }

    /// The store, to read or write it as no session.
    pub fn store_mut(&mut self) -> &mut S {
        match &mut self.store {
            Store::Plain(store) => store,
            Store::Permitted(view) => view.inner_mut().inner_mut(),
        }
    }

    /// The policy, when there are permissions.
    pub fn policy(&self) -> Option<&P> {
        match &self.store {
            Store::Plain(_) => None,
            Store::Permitted(view) => Some(view.policy()),
        }
    }

    pub fn into_inner(self) -> S {
        match self.store {
            Store::Plain(store) => store,
            Store::Permitted(view) => view.into_inner().into_inner(),
        }
    }

    /// Opens a session of `subject`, by default the one of
    /// [`with_subject`](Self::with_subject).
    pub fn open(&mut self, subject: Option<u32>) -> SessionId {
        let id = SessionId(self.next);
        self.next += 1;
        self.sessions.insert(
            id,
            Session {
                subject: subject.unwrap_or(self.subject),
                created: HashSet::new(),
            },
        );
        id
    }

    /// Closes a session.
    pub fn close(&mut self, session: SessionId) {
        self.sessions.remove(&session);
    }

    /// The sessions that are open.
    pub fn sessions(&self) -> Vec<SessionId> {
        let mut sessions: Vec<SessionId> = self.sessions.keys().copied().collect();
        sessions.sort();
        sessions
    }

    /// The subject the requests of `session` run as, while it is open.
    pub fn subject(&self, session: SessionId) -> Option<u32> {
        self.sessions.get(&session).map(|session| session.subject)
    }

    /// The links `session` created, while they are its own.
    pub fn created(&self, session: SessionId) -> Option<&HashSet<u32>> {
        self.sessions.get(&session).map(|session| &session.created)
    }

    /// Answers a request written as LiNo text, as the text protocol does.
    pub fn request(&mut self, session: SessionId, text: &str) -> String {
        let reply = match parse_document(text) {
            Ok(document) => self.execute(session, &document),
            Err(error) => error_document(&error.to_string()),
        };
        format_document(&reply)
    }

    /// Answers one request document of `session`; a session that is not
    /// open is opened.
    pub fn execute(&mut self, session: SessionId, document: &[LiNo<String>]) -> LinoDocument {
        self.execute_observed(session, document, &mut Vec::new())
    }

    /// Like [`execute`](Self::execute), adding the net change of every link
    /// the request changed in the store to `changed`, with plain numbers:
    /// the cascades and merges a write led to as well, even where the
    /// subject cannot see them. A request undone for failing changed nothing.
    pub fn execute_observed(
        &mut self,
        session: SessionId,
        document: &[LiNo<String>],
        changed: &mut Vec<Change>,
    ) -> LinoDocument {
        self.run(session, document, changed, &mut Vec::new())
    }

    /// Like [`execute`](Self::execute) on sessions that read requests as
    /// triplets, adding every triplet the request changed to `changed`, with
    /// plain numbers: those it took along too, even where the subject cannot
    /// see them. A request undone for failing changed nothing.
    pub fn execute_triplets_observed(
        &mut self,
        session: SessionId,
        document: &[LiNo<String>],
        changed: &mut Vec<TripletChange>,
    ) -> LinoDocument {
        self.run(session, document, &mut Vec::new(), changed)
    }

    fn run(
        &mut self,
        session: SessionId,
        document: &[LiNo<String>],
        changed: &mut Vec<Change>,
        triplets_changed: &mut Vec<TripletChange>,
    ) -> LinoDocument {
        let mut state = self.sessions.remove(&session).unwrap_or(Session {
            subject: self.subject,
            created: HashSet::new(),
        });
        let reply = self.execute_as(&mut state, document, changed, triplets_changed);
        self.sessions.insert(session, state);
        reply
    }

    /// Whether the subject of `session` may read `link`: always, without
    /// permissions; a session that is not open is a guest.
    pub fn can_read(&mut self, session: SessionId, link: &Link) -> bool {
        let view = match &mut self.store {
            Store::Plain(_) => return true,
            Store::Permitted(view) => view,
        };
        let (subject, created) = match self.sessions.get_mut(&session) {
            Some(state) => (state.subject, std::mem::take(&mut state.created)),
            None => (self.subject, HashSet::new()),
        };
        view.set_subject(subject);
        let created = view.replace_created(created);
        let readable = if self.triplets.is_some() {
            view.can_read_triplet(link)
        } else {
            view.can_read(link)
        };
        let created = view.replace_created(created);
        if let Some(state) = self.sessions.get_mut(&session) {
            state.created = created;
        }
        readable
    }

    fn execute_as(
        &mut self,
        session: &mut Session,
        document: &[LiNo<String>],
        changed: &mut Vec<Change>,
        triplets_changed: &mut Vec<TripletChange>,
    ) -> LinoDocument {
        if let [LiNo::Link {
            id: Some(id),
            values,
        }] = document
        {
            if id == "auth" {
                return self.authenticate(session, values);
            }
        }
        let markers = self.triplets;
        let view = match &mut self.store {
            Store::Plain(store) => {
                return match markers {
                    Some(markers) => execute_observed_triplets_request(
                        &mut TripletsOverDoublets::with_markers(store, markers),
                        document,
                        triplets_changed,
                    ),
                    None => execute_observed_request(store, &self.processor, document, changed),
                };
            }
            Store::Permitted(view) => view,
        };
        view.set_subject(session.subject);
        view.replace_created(std::mem::take(&mut session.created));
        view.inner_mut().begin();
        let processor = &self.processor;
        let observed = triplets_changed.len();
        let outcome = panic::catch_unwind(AssertUnwindSafe(|| match markers {
            Some(markers) => triplets_request(view, markers, document, triplets_changed),
            None => request(view, document, |view, document| {
                execute_request(view, processor, document)
            }),
        }));
        let reply = match outcome {
            Ok(reply) if error_message(&reply).is_none() => {
                let deleted: HashSet<u32> = view
                    .inner()
                    .log()
                    .changes()
                    .iter()
                    .filter_map(|change| match change {
                        (Some(before), None) => Some(before.index),
                        _ => None,
                    })
                    .collect();
                let changes = view
                    .inner()
                    .log()
                    .changes()
                    .iter()
                    .map(|(before, after)| {
                        (
                            before.unwrap_or_else(Link::null),
                            after.unwrap_or_else(Link::null),
                        )
                    })
                    .collect();
                changed.extend(
                    simplify_changes(changes)
                        .into_iter()
                        .filter(|(before, after)| before != after),
                );
                view.inner_mut().commit();
                // The links are gone: no other session owns them any more.
                for other in self.sessions.values_mut() {
                    other.created.retain(|index| !deleted.contains(index));
                }
                reply
            }
            Ok(reply) => match undo(view) {
                Ok(()) => {
                    triplets_changed.truncate(observed);
                    reply
                }
                Err(error) => error_document(&format!(
                    "{}; undoing it failed: {error:#}",
                    error_message(&reply).unwrap_or_default()
                )),
            },
            Err(panicked) => {
                let _ = undo(view);
                session.created = view.replace_created(HashSet::new());
                panic::resume_unwind(panicked);
            }
        };
        session.created = view.replace_created(HashSet::new());
        reply
    }

    fn authenticate(&mut self, session: &mut Session, values: &[LiNo<String>]) -> LinoDocument {
        let found = match (values, self.authenticate.as_mut()) {
            ([LiNo::Ref(token)], Some(authenticate)) => {
                let store: &mut dyn NamedTypeLinks = match &mut self.store {
                    Store::Plain(store) => store,
                    Store::Permitted(view) => view.inner_mut().inner_mut(),
                };
                authenticate(store, token).ok().flatten()
            }
            _ => None,
        };
        match found {
            Some(subject) => {
                session.subject = subject;
                vec![LiNo::Link {
                    id: Some("subject".to_string()),
                    values: vec![LiNo::Ref(subject.to_string())],
                }]
            }
            None => error_document("Forbidden: authentication failed"),
        }
    }
}

/// Undoes the request, and forgets the links it created.
fn undo<S: NamedTypeLinks, P: AccessPolicy>(
    view: &mut PermissionsDecorator<UndoableLinksDecorator<S>, P>,
) -> Result<()> {
    for created in view.inner_mut().rollback()? {
        view.forget(created);
    }
    Ok(())
}

/// A request as the subject sees the store: names of links it cannot read
/// are not found, and `run` answers everything else.
fn request<L: NamedTypeLinks, P: AccessPolicy>(
    view: &mut PermissionsDecorator<L, P>,
    document: &[LiNo<String>],
    run: impl FnOnce(&mut PermissionsDecorator<L, P>, &[LiNo<String>]) -> LinoDocument,
) -> LinoDocument {
    let operation = match LinksOperation::from_document(document) {
        Ok(operation) => operation,
        Err(error) => return error_document(&error.to_string()),
    };
    match operation {
        Some(LinksOperation::GetName(link)) if !view.exists(link) => Vec::new(),
        Some(LinksOperation::GetByName(name)) => match view.get_by_name(&name) {
            Ok(Some(link)) if link != 0 && view.exists(link) => run(view, document),
            Ok(_) => Vec::new(),
            Err(error) => error_document(&format!("{error:#}")),
        },
        _ => run(view, document),
    }
}

/// A request over triplets as the subject sees the store: the permissions move
/// above the triplets, which write their wrappers and pairs straight to the
/// store below, still inside the transaction of the request.
fn triplets_request<S: NamedTypeLinks, P: AccessPolicy>(
    view: &mut PermissionsDecorator<UndoableLinksDecorator<S>, P>,
    markers: TripletMarkers,
    document: &[LiNo<String>],
    changed: &mut Vec<TripletChange>,
) -> LinoDocument {
    let subject = view.subject();
    let created = view.replace_created(HashSet::new());
    let (links, policy) = view.parts_mut();
    let mut layer = PermissionsDecorator::new(
        TripletsOverDoublets::with_markers(links, markers),
        policy,
        subject,
    )
    .with_created(created);
    // A triplet operation holds four parts where one over doublets holds three:
    // only what is left, the names, is for `request` to look at.
    let reply = if matches!(TripletsOperation::from_document(document), Ok(None)) {
        request(&mut layer, document, |layer, document| {
            execute_observed_triplets_request(layer, document, changed)
        })
    } else {
        execute_observed_triplets_request(&mut layer, document, changed)
    };
    let created = layer.replace_created(HashSet::new());
    view.replace_created(created);
    reply
}
