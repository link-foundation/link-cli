//! Session/permission composition must retain the checked store's admission.
use anyhow::Result;
use link_cli::link_storage::ChangeObserver;
use link_cli::protocol::{error_message, parse_document, query_document, LinksSessions};
use link_cli::query_options::QueryLimits;
use link_cli::{
    Link, LinkStorage, NamedTypeLinks, PermissionPolicy, PermissionsDecorator,
    UndoableLinksDecorator,
};

struct Checked(LinkStorage);
impl NamedTypeLinks for Checked {
    fn query_limits(&self) -> Option<QueryLimits> {
        Some(QueryLimits {
            max_bytes: 64,
            max_depth: 4,
            ..QueryLimits::causal()
        })
    }
    fn create(&mut self, s: u32, t: u32) -> u32 {
        self.0.create(s, t)
    }
    fn ensure_created(&mut self, id: u32) -> u32 {
        self.0.ensure_created(id)
    }
    fn get_link(&mut self, id: u32) -> Option<Link> {
        self.0.get_link(id)
    }
    fn exists(&mut self, id: u32) -> bool {
        self.0.exists(id)
    }
    fn update_observed(
        &mut self,
        id: u32,
        s: u32,
        t: u32,
        changed: ChangeObserver<'_>,
    ) -> Result<Link> {
        self.0.update_observed(id, s, t, changed)
    }
    fn delete_observed(&mut self, id: u32, changed: ChangeObserver<'_>) -> Result<Link> {
        self.0.delete_observed(id, changed)
    }
    fn all_links(&mut self) -> Vec<Link> {
        self.0.all_links()
    }
    fn search(&mut self, s: u32, t: u32) -> Option<u32> {
        self.0.search(s, t)
    }
    fn get_or_create(&mut self, s: u32, t: u32) -> u32 {
        self.0.get_or_create(s, t)
    }
    fn get_name(&mut self, id: u32) -> Result<Option<String>> {
        NamedTypeLinks::get_name(&mut self.0, id)
    }
    fn set_name(&mut self, id: u32, name: &str) -> Result<u32> {
        NamedTypeLinks::set_name(&mut self.0, id, name)
    }
    fn get_by_name(&mut self, name: &str) -> Result<Option<u32>> {
        NamedTypeLinks::get_by_name(&mut self.0, name)
    }
    fn remove_name(&mut self, id: u32) -> Result<()> {
        NamedTypeLinks::remove_name(&mut self.0, id)
    }
    fn save(&mut self) -> Result<()> {
        self.0.save()
    }
}

#[test]
fn permission_and_undo_layers_preserve_checked_query_admission() -> Result<()> {
    let mut store = Checked(LinkStorage::in_memory(false));
    let policy = PermissionPolicy::in_data(&mut store)?;
    let undo = UndoableLinksDecorator::new(store);
    let mut view = PermissionsDecorator::new(undo, policy, 0);
    assert_eq!(view.query_limits().unwrap().max_depth, 4);
    // The empty export must pass through the borrowed facade used by sessions.
    let mut sessions = LinksSessions::new(&mut view);
    let session = sessions.open(None);
    let document = query_document(&"x".repeat(65));
    assert!(error_message(&sessions.execute(session, &document))
        .unwrap()
        .contains("byte limit"));
    let text = format!("{}x{}", "(n: ".repeat(12), ")".repeat(12));
    assert!(sessions.request(session, &text).contains("error:"));
    let deep = parse_document(&text).unwrap();
    assert!(error_message(&sessions.execute(session, &deep))
        .unwrap()
        .contains("depth limit"));
    Ok(())
}

#[test]
fn lossless_envelope_keeps_observed_changes_for_subscribers() {
    let mut sessions = LinksSessions::new(LinkStorage::in_memory(false));
    let session = sessions.open(None);
    let mut changes = Vec::new();
    let reply = sessions.execute_observed(session, &query_document("() ((1: 1 1))"), &mut changes);
    assert!(error_message(&reply).is_none());
    assert_eq!(changes, vec![(Link::null(), Link::new(1, 1, 1))]);
}
