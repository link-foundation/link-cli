//! The permission layer with the other layers (issue #149): over a union, a
//! remap, transactions and version control, under them, and with roots and
//! type systems other than the named `Type` point. The world is the one of
//! `permissions_decorator_tests.rs`; the C# twin is
//! `csharp/Foundation.Data.Doublets.Cli.Tests/PermissionsIntegrationTests.cs`,
//! case for case.

use anyhow::Result;
use link_cli::{
    AccessPolicy, CommitMode, FileTransitionLog, Forbidden, GenericTransactionsDecorator, Link,
    LinkStorage, LinksStorage, LogRetentionPolicy, NamedLinksStorage, NamedTypeLinks,
    NamedTypesDecorator, OffsetAddressMap, PermissionAccess, PermissionMarkers, PermissionPolicy,
    PermissionsDecorator, RemapLinks, TransactionsDecorator, TypeMarkers, Types, TypesDecorator,
    UnionLinks, VersionControlDecorator,
};
use tempfile::TempDir;

#[path = "common/permissions_world.rs"]
mod world;

#[path = "common/union_world.rs"]
mod union_world;

use union_world::{all, make};
use world::{make_world, World};

/// Where a server's network reads in a union with a client's.
const SERVER_OFFSET: u32 = 1 << 24;

fn s(address: u32) -> u32 {
    address + SERVER_OFFSET
}

fn indices(links: &mut dyn NamedTypeLinks) -> Vec<u32> {
    all(links).into_iter().map(|(index, _, _)| index).collect()
}

/// `forbidden`, `not-found`, or `None` when `result` is no error. A refusal
/// that went through the transactions layer reads `Storage error: Forbidden: …`.
fn error_of<T>(result: Result<T>) -> Option<&'static str> {
    let error = result.err()?;
    let text = error.to_string();
    if error.downcast_ref::<Forbidden>().is_some() || text.contains("Forbidden: ") {
        Some("forbidden")
    } else if text.starts_with("Link not found") {
        Some("not-found")
    } else {
        panic!("an unexpected error: {text}")
    }
}

/// A new link `(l: type l)`.
fn typed(links: &mut dyn NamedTypeLinks, type_: u32) -> Result<u32> {
    let link = links.try_create(0, 0)?;
    links.update(link, type_, link)?;
    Ok(link)
}

/// A client's network and a server's, the server moved out of the client's way.
fn client_and_server<'a>(
    client: &'a mut LinkStorage,
    server: &'a mut LinkStorage,
) -> Result<UnionLinks<'a>> {
    UnionLinks::new(vec![
        Box::new(client),
        Box::new(RemapLinks::new(
            server,
            OffsetAddressMap::new(SERVER_OFFSET),
        )),
    ])
}

fn member<'u>(union: &'u mut UnionLinks<'_>, at: usize) -> &'u mut dyn NamedTypeLinks {
    &mut *union.members_mut()[at]
}

#[test]
fn permissions_over_a_union_restrict_every_member() -> Result<()> {
    let mut client = LinkStorage::in_memory(false);
    let mut server = LinkStorage::in_memory(false);
    let (t, v, markers, _) = make_world(&mut server, None)?;
    let mut union = client_and_server(&mut client, &mut server)?;
    // Markers, types and rules are found through the union, in whichever member they are.
    let mut policy = PermissionPolicy::in_data(&mut union)?;
    assert_eq!(s(markers.allow_read), policy.markers().allow_read);
    let mut view = PermissionsDecorator::new(&mut union, &mut policy, s(v["alice"]));
    assert_eq!(
        vec![s(t["Shared"]), s(v["note"]), s(v["shared"])],
        indices(&mut view)
    );

    // Links of the client, typed by types of the server.
    let draft = typed(&mut view, s(t["Note"]))?;
    view.set_subject(s(v["boss"]));
    let hidden = typed(&mut view, s(t["Secret"]))?;
    view.set_subject(s(v["alice"]));
    assert_eq!(
        vec![
            (draft, s(t["Note"]), draft),
            (hidden, s(t["Secret"]), hidden)
        ],
        all(member(view.inner_mut(), 0))
    );
    assert_eq!(
        vec![draft, s(t["Shared"]), s(v["note"]), s(v["shared"])],
        indices(&mut view)
    );
    assert_eq!(Some("not-found"), error_of(view.delete(hidden)));
    assert_eq!(Some("not-found"), error_of(view.delete(s(v["secret"]))));
    assert_eq!(
        Some("forbidden"),
        error_of(view.update(draft, s(t["Secret"]), draft))
    );
    view.set_subject(0);
    assert_eq!(Some("forbidden"), error_of(typed(&mut view, s(t["Note"]))));

    // A rule of the client counts as much as the rules of the server.
    view.set_subject(s(v["boss"]));
    let markers = view.policy().markers();
    PermissionPolicy::allow(
        &mut view,
        markers,
        PermissionAccess::Read,
        s(t["User"]),
        s(t["Secret"]),
    )?;
    view.set_subject(s(v["alice"]));
    assert!(indices(&mut view).contains(&hidden));
    assert!(indices(&mut view).contains(&s(v["secret"])));
    drop(view);
    let granted: Vec<(u32, u32)> = policy
        .granted(&mut union, markers.allow_read)
        .into_iter()
        .filter(|&(_, object)| object == s(t["Secret"]))
        .collect();
    assert_eq!(vec![(s(t["User"]), s(t["Secret"]))], granted);
    Ok(())
}

#[test]
fn a_refused_delete_over_a_union_comes_back_in_its_member() -> Result<()> {
    let mut client = LinkStorage::in_memory(false);
    let mut server = LinkStorage::in_memory(false);
    let (t, v, _, _) = make_world(&mut server, None)?;
    // A hidden link of the server refers to alice's note: deleting the note deletes it.
    let hidden_use = {
        let mut policy = PermissionPolicy::in_data(&mut server)?;
        let mut boss = PermissionsDecorator::new(&mut server, &mut policy, v["boss"]);
        let hidden_use = boss.try_create(0, 0)?;
        boss.update(hidden_use, t["Secret"], v["note"])?;
        hidden_use
    };
    let before = all(&mut server);
    let mut union = client_and_server(&mut client, &mut server)?;
    let mut policy = PermissionPolicy::in_data(&mut union)?;
    let mut alice = PermissionsDecorator::new(&mut union, &mut policy, s(v["alice"]));
    assert_eq!(Some("forbidden"), error_of(alice.delete(s(v["note"]))));
    drop(alice);
    // Both come back in the server, at their addresses, and the client gets nothing.
    assert_eq!(Some("note".to_string()), union.get_name(s(v["note"]))?);
    assert_eq!(Some(1), union.owner_of(s(hidden_use)));
    drop(union);
    assert_eq!(before, all(&mut server));
    assert!(all(&mut client).is_empty());
    Ok(())
}

#[test]
fn permissions_under_a_union_restrict_their_member_only() -> Result<()> {
    let mut client = LinkStorage::in_memory(false);
    let mut server = LinkStorage::in_memory(false);
    let (t, v, _, _) = make_world(&mut server, None)?;
    let mut remote = RemapLinks::new(&mut server, OffsetAddressMap::new(SERVER_OFFSET));
    let policy = PermissionPolicy::in_data(&mut remote)?;
    let seen = PermissionsDecorator::new(remote, policy, s(v["alice"]));
    let own = make(&mut client, 1, 1)?;
    let mut union = UnionLinks::new(vec![Box::new(&mut client), Box::new(seen)])?;
    assert_eq!(
        vec![own, s(t["Shared"]), s(v["note"]), s(v["shared"])],
        indices(&mut union)
    );
    assert!(!union.exists(s(v["secret"])));
    assert_eq!(None, union.owner_of(s(v["secret"])));
    assert_eq!(
        Some("forbidden"),
        error_of(union.update(s(v["note"]), s(t["Secret"]), s(v["note"])))
    );
    union.update(own, s(t["Secret"]), own)?;
    drop(union);
    assert_eq!(vec![(own, s(t["Secret"]), own)], all(&mut client));
    Ok(())
}

#[test]
fn a_remap_over_permissions_sees_what_the_subject_may() -> Result<()> {
    let mut w = World::new(true)?;
    let (t, v) = (w.t.clone(), w.v.clone());
    let mut remap = RemapLinks::new(w.as_(v["alice"]), OffsetAddressMap::new(1000));
    let expected: Vec<u32> = [t["Shared"], v["note"], v["shared"]]
        .iter()
        .map(|index| index + 1000)
        .collect();
    assert_eq!(expected, indices(&mut remap));
    assert_eq!(
        Some("forbidden"),
        error_of(remap.update(v["note"] + 1000, t["Secret"] + 1000, v["note"] + 1000))
    );
    assert_eq!(
        Some("not-found"),
        error_of(remap.delete(v["secret"] + 1000))
    );
    assert_eq!(Some(v["note"] + 1000), remap.get_by_name("note")?);
    Ok(())
}

/// A new store of named links in `dir`.
fn named_store(dir: &TempDir, name: &str) -> Result<NamedTypesDecorator> {
    NamedTypesDecorator::new(dir.path().join(format!("{name}.links")), false)
}

fn transactions_in(dir: &TempDir) -> Result<TransactionsDecorator> {
    Ok(TransactionsDecorator::new(
        named_store(dir, "data")?,
        named_store(dir, "log")?,
        LogRetentionPolicy::default(),
        CommitMode::default(),
        false,
    )?)
}

#[test]
fn permissions_over_transactions_log_only_what_they_allow() -> Result<()> {
    let dir = TempDir::new()?;
    let mut transactions = transactions_in(&dir)?;
    let (t, v, _, _) = make_world(&mut transactions, None)?;
    let mut policy = PermissionPolicy::in_data(&mut transactions)?;
    let mut alice = PermissionsDecorator::new(&mut transactions, &mut policy, v["alice"]);
    assert_eq!(
        vec![t["Shared"], v["note"], v["shared"]],
        indices(&mut alice)
    );
    let logged = alice.inner().log().len();
    assert_eq!(
        Some("forbidden"),
        error_of(alice.update(v["note"], t["Secret"], v["note"]))
    );
    assert_eq!(Some("not-found"), error_of(alice.delete(v["secret"])));
    assert_eq!(logged, alice.inner().log().len());
    let draft = typed(&mut alice, t["Note"])?;
    assert!(alice.inner().log().len() > logged);
    drop(alice);
    assert_eq!(
        Some(Link::new(draft, t["Note"], draft)),
        transactions.get(draft).copied()
    );
    Ok(())
}

#[test]
fn transactions_over_permissions_log_only_what_they_allow() -> Result<()> {
    let mut w = World::new(false)?;
    let (t, v) = (w.t.clone(), w.v.clone());
    let dir = TempDir::new()?;
    let log = FileTransitionLog::open(dir.path().join("log.transitions"))?;
    let mut transactions = GenericTransactionsDecorator::new(
        NamedLinksStorage::new(w.as_(v["alice"])),
        log,
        LogRetentionPolicy::default(),
        CommitMode::default(),
        false,
    )?;
    let mut seen: Vec<u32> = transactions
        .inner()
        .all_links()
        .into_iter()
        .map(|link| link.index)
        .collect();
    seen.sort_unstable();
    assert_eq!(vec![t["Shared"], v["note"], v["shared"]], seen);
    assert_eq!(
        Some("forbidden"),
        error_of(
            transactions
                .update(v["note"], t["Secret"], v["note"])
                .map_err(anyhow::Error::from)
        )
    );
    assert!(transactions.log().is_empty());
    let draft = transactions.create(0, 0)?;
    transactions.update(draft, t["Note"], draft)?;
    assert!(!transactions.log().is_empty());
    drop(transactions);
    assert_eq!(Link::new(draft, t["Note"], draft), w.link(draft));
    Ok(())
}

#[test]
fn permissions_over_version_control_see_every_branch_through_the_rules() -> Result<()> {
    let dir = TempDir::new()?;
    let transactions = transactions_in(&dir)?;
    let mut versions = VersionControlDecorator::new(transactions, named_store(&dir, "vc")?, false)?;
    let (t, v, _, _) = make_world(&mut versions, None)?;
    let mut policy = PermissionPolicy::in_data(&mut versions)?;
    let mut alice = PermissionsDecorator::new(&mut versions, &mut policy, v["alice"]);
    assert_eq!(
        vec![t["Shared"], v["note"], v["shared"]],
        indices(&mut alice)
    );
    assert_eq!(
        Some("forbidden"),
        error_of(alice.update(v["note"], t["Secret"], v["note"]))
    );
    let draft = typed(&mut alice, t["Note"])?;
    assert!(indices(&mut alice).contains(&draft));
    drop(alice);
    assert_eq!(
        Some(Link::new(draft, t["Note"], draft)),
        versions.get_link(draft)
    );
    Ok(())
}

#[test]
fn roots_and_markers_may_be_unnamed_links_of_any_address() -> Result<()> {
    let mut data = LinkStorage::in_memory(false);
    let mut rules = LinkStorage::in_memory(false);
    // A root of its own, not the point named Type: any link may be one.
    let root = make(&mut data, 1, 1)?;
    let markers = PermissionMarkers {
        allow_read: make(&mut data, 2, 2)?,
        allow_write: make(&mut data, 3, 3)?,
        anything: make(&mut data, 4, 4)?,
    };
    let mut types = TypesDecorator::new(&mut data, root, None);
    let user = types.define_root()?;
    let note = types.define_root()?;
    let alice = types.instance(user)?;
    let mine = types.instance(note)?;
    let type_markers = types.markers();
    let other = typed(&mut data, root)?;
    PermissionPolicy::allow(&mut rules, markers, PermissionAccess::Write, user, note)?;
    let policy = PermissionPolicy::new(markers, type_markers, Some(Box::new(rules)));
    let mut view = PermissionsDecorator::new(&mut data, policy, alice);
    assert_eq!(vec![mine], indices(&mut view));
    assert_eq!(Some("not-found"), error_of(view.delete(other)));
    drop(view);
    assert_eq!(None, NamedTypeLinks::get_by_name(&mut data, "Type")?);
    assert_eq!(None, NamedTypeLinks::get_by_name(&mut data, "AllowWrite")?);
    Ok(())
}

/// A type system of its own: a link is of the type that is its source, and
/// nothing extends anything. The rules are the stored ones.
struct SourceTyped(PermissionPolicy);

impl SourceTyped {
    fn is_subject(data: &mut dyn NamedTypeLinks, anything: u32, subject: u32, type_: u32) -> bool {
        type_ == anything
            || (subject != 0
                && (subject == type_
                    || data
                        .get_link(subject)
                        .is_some_and(|link| link.source == type_)))
    }

    fn allows(
        &mut self,
        data: &mut dyn NamedTypeLinks,
        marker: u32,
        subject: u32,
        link: &Link,
    ) -> bool {
        let anything = self.0.markers().anything;
        self.0
            .granted(data, marker)
            .into_iter()
            .any(|(rule_subject, object)| {
                Self::is_subject(data, anything, subject, rule_subject)
                    && (object == anything || link.source == object)
            })
    }
}

impl AccessPolicy for SourceTyped {
    fn markers(&self) -> PermissionMarkers {
        self.0.markers()
    }

    fn can_read(&mut self, data: &mut dyn NamedTypeLinks, subject: u32, link: &Link) -> bool {
        let markers = self.markers();
        self.allows(data, markers.allow_read, subject, link)
            || self.allows(data, markers.allow_write, subject, link)
    }

    fn can_write(&mut self, data: &mut dyn NamedTypeLinks, subject: u32, link: &Link) -> bool {
        let marker = self.markers().allow_write;
        self.allows(data, marker, subject, link)
    }

    fn can_create(&mut self, data: &mut dyn NamedTypeLinks, subject: u32) -> bool {
        let markers = self.markers();
        self.0
            .granted(data, markers.allow_write)
            .into_iter()
            .any(|(rule_subject, _)| {
                Self::is_subject(data, markers.anything, subject, rule_subject)
            })
    }
}

#[test]
fn a_policy_may_bring_a_type_system_of_its_own() -> Result<()> {
    let mut data = LinkStorage::in_memory(false);
    let markers = PermissionMarkers::ensure(&mut data)?;
    let person = make(&mut data, 0, 0)?;
    let post = make(&mut data, 0, 0)?;
    data.update(person, person, person)?;
    data.update(post, post, post)?;
    let alice = make(&mut data, person, 0)?;
    let mine = make(&mut data, post, alice)?;
    let other = make(&mut data, person, person)?;
    PermissionPolicy::allow(&mut data, markers, PermissionAccess::Write, person, post)?;
    let no_types = TypeMarkers {
        root: 0,
        extends: None,
    };
    let policy = SourceTyped(PermissionPolicy::new(markers, no_types, None));
    let mut view = PermissionsDecorator::new(&mut data, policy, alice);
    // The type of posts is a post too: it starts at itself.
    assert_eq!(vec![post, mine], indices(&mut view));
    assert_eq!(Some("not-found"), error_of(view.delete(other)));
    let draft = make(&mut view, post, person)?;
    assert_eq!(
        Some("forbidden"),
        error_of(view.update(draft, person, post))
    );
    Ok(())
}

/// A policy of no rules at all: everyone reads everything, a link is written
/// by the subject it starts at.
struct OwnerPolicy;

impl AccessPolicy for OwnerPolicy {
    fn markers(&self) -> PermissionMarkers {
        PermissionMarkers {
            allow_read: 0,
            allow_write: 0,
            anything: 0,
        }
    }

    fn can_read(&mut self, _: &mut dyn NamedTypeLinks, subject: u32, _: &Link) -> bool {
        subject != 0
    }

    fn can_write(&mut self, _: &mut dyn NamedTypeLinks, subject: u32, link: &Link) -> bool {
        subject != 0 && link.source == subject
    }

    fn can_create(&mut self, _: &mut dyn NamedTypeLinks, subject: u32) -> bool {
        subject != 0
    }
}

#[test]
fn a_policy_may_keep_no_rules_at_all() -> Result<()> {
    let mut data = LinkStorage::in_memory(false);
    let alice = make(&mut data, 1, 1)?;
    let bob = make(&mut data, 2, 2)?;
    let post = make(&mut data, bob, alice)?;
    let mut as_alice = PermissionsDecorator::new(&mut data, OwnerPolicy, alice);
    assert_eq!(vec![alice, bob, post], indices(&mut as_alice));
    assert_eq!(Some("forbidden"), error_of(as_alice.delete(post)));
    let mine = make(&mut as_alice, alice, bob)?;
    assert_eq!(
        Some(Link::new(mine, alice, bob)),
        as_alice.inner_mut().get_link(mine)
    );
    assert_eq!(Some("forbidden"), error_of(as_alice.update(mine, bob, bob)));
    drop(as_alice);
    let mut guest = PermissionsDecorator::new(&mut data, OwnerPolicy, 0);
    assert_eq!(Some("forbidden"), error_of(make(&mut guest, 1, 1)));
    assert!(all(&mut guest).is_empty());
    Ok(())
}
