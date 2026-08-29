//! Persistent transformation triggers.
//!
//! A *trigger* is a stored LiNo substitution query that the CLI replays after
//! every write, turning a one-off transformation into a standing rule. This is
//! the Rust port of
//! `Foundation.Data.Doublets.Cli.PersistentTransformationDecorator`, and it
//! keeps the same on-disk shape so the two implementations can read each
//! other's trigger databases:
//!
//! ```text
//! (Always ((Condition <condition text>) (Substitution <substitution text>)))
//! (Once   ((Condition <condition text>) (Substitution <substitution text>)))
//! ```
//!
//! `Condition`, `Substitution`, `Type`, `Trigger`, `Once` and `Always` are
//! named points; the condition and substitution texts are named points too,
//! whose names carry the [`INTERNAL_NAME_PREFIX`] so they cannot collide with
//! user-visible names.
//!
//! # Where the triggers live
//!
//! [`TriggerStore`] decides that: [`TriggerStore::Sidecar`] keeps them in a
//! companion database (`<db>.triggers.links` by default, see
//! [`make_triggers_database_filename`]), while [`TriggerStore::Embedded`]
//! stores them in the decorated database itself — the `--embed-triggers` mode.
//!
//! # Extension points
//!
//! The decorator is generic over any [`NamedTypeLinks`], so it composes with
//! the plain store, the transactions layer and the version-control layer
//! alike, and an embedder can stack it wherever it wants in its own chain. The
//! parsing (
//! [`PersistentTransformationQuery`]), the stored form
//! ([`PersistentTransformation`]) and the store selection ([`TriggerStore`])
//! are all public so custom CLIs can inspect, migrate or generate triggers
//! without going through this decorator at all.

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};

use crate::link::Link;
use crate::lino_link::LinoLink;
use crate::named_type_links::{escape_lino_reference, NamedTypeLinks};
use crate::named_types::NamedTypesDecorator;
use crate::parser::Parser;
use crate::query_processor::QueryProcessor;

/// Prefix of the internal names used for stored condition and substitution
/// texts. It keeps trigger bookkeeping distinguishable from user names even in
/// [`TriggerStore::Embedded`] mode, where both share one namespace.
pub const INTERNAL_NAME_PREFIX: &str = "__persistent_transformation:";

const MISSING_PARTS: &str =
    "Persistent transformation query must contain a condition and a substitution.";

/// How long a stored trigger lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PersistentTransformationKind {
    /// Applied once, then removed as soon as an application produced changes.
    Once,
    /// Applied after every write, indefinitely.
    Always,
}

impl fmt::Display for PersistentTransformationKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Self::Once => "Once",
            Self::Always => "Always",
        };
        formatter.write_str(text)
    }
}

/// A trigger as it is stored in a links database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistentTransformation {
    /// Address of the `(kind payload)` link that roots this trigger.
    pub root: u32,
    pub kind: PersistentTransformationKind,
    /// The condition (left) half of the substitution query.
    pub condition: String,
    /// The substitution (right) half of the substitution query.
    pub substitution: String,
}

impl PersistentTransformation {
    /// The query that gets replayed after every write.
    pub fn query(&self) -> String {
        format!("({} {})", self.condition, self.substitution)
    }
}

/// A trigger query split into its condition and substitution halves.
///
/// Both halves are re-formatted from the parse tree rather than kept as raw
/// input, so two spellings of the same query (`((1: 1 1)) ((1: 1 2))` and
/// `(((1: 1 1)) ((1: 1 2)))`) normalise to the same stored text and therefore
/// to the same trigger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistentTransformationQuery {
    pub condition: String,
    pub substitution: String,
}

impl PersistentTransformationQuery {
    /// Parses `query` into its two halves.
    ///
    /// Both the wrapped form `((condition) (substitution))` and the bare form
    /// `(condition) (substitution)` are accepted, matching the C# parser.
    pub fn parse(query: &str) -> Result<Self> {
        let parsed = Parser::new().parse(query)?;
        let outer = parsed.first().ok_or_else(|| anyhow!(MISSING_PARTS))?;

        let (condition, substitution) = match outer.values.as_deref() {
            Some(values) if values.len() >= 2 => (&values[0], &values[1]),
            _ if parsed.len() >= 2 => (&parsed[0], &parsed[1]),
            _ => return Err(anyhow!(MISSING_PARTS)),
        };

        Ok(Self {
            condition: format_lino(condition),
            substitution: format_lino(substitution),
        })
    }

    /// The normalised `(condition substitution)` query text.
    pub fn query(&self) -> String {
        format!("({} {})", self.condition, self.substitution)
    }
}

/// Renders a parsed LiNo link back to source text.
///
/// Mirrors `PersistentTransformationQuery.Format` in C#: a link without values
/// is just its (escaped) identifier, a link without an identifier is
/// `(values)`, and a link with both is `(id: values)`.
fn format_lino(link: &LinoLink) -> String {
    let values = link.values.as_deref().unwrap_or(&[]);
    let id = link.id.as_deref().unwrap_or_default();

    if values.is_empty() {
        return if id.is_empty() {
            "()".to_string()
        } else {
            escape_lino_reference(id)
        };
    }

    let rendered = values.iter().map(format_lino).collect::<Vec<_>>().join(" ");

    if id.is_empty() {
        format!("({rendered})")
    } else {
        format!("({}: {})", escape_lino_reference(id), rendered)
    }
}

/// Conventional sidecar filename for the trigger store: `<db>.triggers.links`.
pub fn make_triggers_database_filename<P: AsRef<Path>>(database_filename: P) -> PathBuf {
    let path = database_filename.as_ref();
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default();
    let name = format!("{stem}.triggers.links");
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.join(name),
        _ => PathBuf::from(name),
    }
}

/// Where a [`PersistentTransformationDecorator`] keeps its triggers.
pub enum TriggerStore {
    /// In the decorated database itself (`--embed-triggers`).
    Embedded,
    /// In a separate companion database (the default).
    Sidecar(Box<NamedTypesDecorator>),
}

impl TriggerStore {
    /// Opens a sidecar store at `path`.
    pub fn sidecar<P: AsRef<Path>>(path: P, trace: bool) -> Result<Self> {
        Ok(Self::Sidecar(Box::new(NamedTypesDecorator::new(
            path, trace,
        )?)))
    }
}

/// The schema points a trigger is built from.
///
/// `Type` and `Trigger` are part of the stored schema too, but they only
/// classify the other points and are never dereferenced while reading or
/// writing a trigger, so they are not carried here.
#[derive(Debug, Clone, Copy)]
struct TriggerSchema {
    once: u32,
    always: u32,
    condition: u32,
    substitution: u32,
}

const SCHEMA_NAMES: [&str; 6] = [
    "Type",
    "Trigger",
    "Once",
    "Always",
    "Condition",
    "Substitution",
];

/// Creates the schema points and the links that relate them, and returns them.
fn ensure_schema<L: NamedTypeLinks + ?Sized>(links: &mut L) -> Result<TriggerSchema> {
    let r#type = links.get_or_create_named("Type")?;
    let trigger = links.get_or_create_named("Trigger")?;
    let once = links.get_or_create_named("Once")?;
    let always = links.get_or_create_named("Always")?;
    let condition = links.get_or_create_named("Condition")?;
    let substitution = links.get_or_create_named("Substitution")?;

    links.get_or_create(r#type, trigger);
    links.get_or_create(trigger, once);
    links.get_or_create(trigger, always);
    links.get_or_create(r#type, condition);
    links.get_or_create(r#type, substitution);

    Ok(TriggerSchema {
        once,
        always,
        condition,
        substitution,
    })
}

/// Reads the schema without creating anything; `None` when any of the six
/// schema points is missing, i.e. when no trigger has ever been stored in
/// `links`.
fn try_get_schema<L: NamedTypeLinks + ?Sized>(links: &mut L) -> Result<Option<TriggerSchema>> {
    let mut ids = [0u32; 6];
    for (slot, name) in ids.iter_mut().zip(SCHEMA_NAMES) {
        match links.get_by_name(name)? {
            Some(id) => *slot = id,
            None => return Ok(None),
        }
    }

    Ok(Some(TriggerSchema {
        once: ids[2],
        always: ids[3],
        condition: ids[4],
        substitution: ids[5],
    }))
}

/// Every well-formed trigger in `links`, ordered by root address.
fn triggers_in<L: NamedTypeLinks + ?Sized>(links: &mut L) -> Result<Vec<PersistentTransformation>> {
    let Some(schema) = try_get_schema(links)? else {
        return Ok(Vec::new());
    };

    let mut all = links.all_links();
    all.sort_by_key(|link| link.index);
    let by_index: HashMap<u32, Link> = all.iter().map(|link| (link.index, *link)).collect();

    let mut triggers = Vec::new();
    for link in &all {
        let kind = if link.source == schema.always {
            PersistentTransformationKind::Always
        } else if link.source == schema.once {
            PersistentTransformationKind::Once
        } else {
            continue;
        };

        let Some(payload) = by_index.get(&link.target) else {
            continue;
        };
        let (Some(condition_record), Some(substitution_record)) =
            (by_index.get(&payload.source), by_index.get(&payload.target))
        else {
            continue;
        };
        if condition_record.source != schema.condition
            || substitution_record.source != schema.substitution
        {
            continue;
        }

        let condition = links.get_name(condition_record.target)?;
        let substitution = links.get_name(substitution_record.target)?;
        let (Some(condition), Some(substitution)) = (
            decode_text_name(condition.as_deref(), "condition"),
            decode_text_name(substitution.as_deref(), "substitution"),
        ) else {
            continue;
        };

        triggers.push(PersistentTransformation {
            root: link.index,
            kind,
            condition,
            substitution,
        });
    }

    Ok(triggers)
}

/// Writes `parsed` into `links` as a trigger of `kind`, returning its root.
///
/// Every part is created through `get_or_create`, so storing the same trigger
/// twice is idempotent and yields the same root.
fn store_trigger_in<L: NamedTypeLinks + ?Sized>(
    links: &mut L,
    kind: PersistentTransformationKind,
    parsed: &PersistentTransformationQuery,
) -> Result<u32> {
    let schema = ensure_schema(links)?;
    let condition_text = links.get_or_create_named(&condition_text_name(&parsed.condition))?;
    let substitution_text =
        links.get_or_create_named(&substitution_text_name(&parsed.substitution))?;
    let condition_record = links.get_or_create(schema.condition, condition_text);
    let substitution_record = links.get_or_create(schema.substitution, substitution_text);
    let payload = links.get_or_create(condition_record, substitution_record);
    let trigger_type = match kind {
        PersistentTransformationKind::Always => schema.always,
        PersistentTransformationKind::Once => schema.once,
    };
    Ok(links.get_or_create(trigger_type, payload))
}

/// Deletes the `(kind payload)` root link, leaving the shared schema and text
/// points in place — exactly like `DeleteTriggerRoot` in C#.
fn delete_trigger_root<L: NamedTypeLinks + ?Sized>(links: &mut L, root: u32) -> Result<bool> {
    if !links.exists(root) {
        return Ok(false);
    }
    links.delete(root)?;
    Ok(true)
}

fn condition_text_name(condition: &str) -> String {
    format!("{INTERNAL_NAME_PREFIX}condition:{condition}")
}

fn substitution_text_name(substitution: &str) -> String {
    format!("{INTERNAL_NAME_PREFIX}substitution:{substitution}")
}

fn decode_text_name(name: Option<&str>, part: &str) -> Option<String> {
    let prefix = format!("{INTERNAL_NAME_PREFIX}{part}:");
    name?.strip_prefix(&prefix).map(str::to_string)
}

/// Runs `$call` against whichever store holds the triggers.
///
/// [`NamedTypeLinks`] has generic default methods and is therefore not object
/// safe, so the two stores cannot be unified behind a trait object; the macro
/// picks the branch instead. The `Embedded` arm borrows `links` while the
/// scrutinee borrows `triggers` — disjoint fields, which the borrow checker
/// accepts.
macro_rules! on_trigger_links {
    ($self:expr, $call:ident($($arg:expr),* $(,)?)) => {
        match $self.triggers {
            TriggerStore::Sidecar(ref mut store) => $call(store.as_mut() $(, $arg)*),
            TriggerStore::Embedded => $call(&mut $self.links $(, $arg)*),
        }
    };
}

/// Replays stored triggers after every write that goes through it.
///
/// Wrap it around any [`NamedTypeLinks`] — the bare store, the transactions
/// decorator, the version-control decorator, or a custom one.
pub struct PersistentTransformationDecorator<L: NamedTypeLinks> {
    links: L,
    triggers: TriggerStore,
    trace: bool,
    applying_triggers: bool,
    suppress_triggers: bool,
    auto_create_missing_references: bool,
    /// First failure raised while applying triggers from an infallible write.
    ///
    /// [`NamedTypeLinks::create`], `ensure_created` and `get_or_create` cannot
    /// report an error, so a failing trigger is parked here and surfaced by the
    /// next fallible operation — at the latest by
    /// [`save`](NamedTypeLinks::save), which the CLI always calls.
    pending_error: Option<anyhow::Error>,
}

impl<L: NamedTypeLinks> PersistentTransformationDecorator<L> {
    pub fn new(links: L, triggers: TriggerStore, trace: bool) -> Self {
        Self {
            links,
            triggers,
            trace,
            applying_triggers: false,
            suppress_triggers: false,
            auto_create_missing_references: false,
            pending_error: None,
        }
    }

    /// Keeps the triggers in the decorated database itself.
    pub fn embedded(links: L, trace: bool) -> Self {
        Self::new(links, TriggerStore::Embedded, trace)
    }

    /// Keeps the triggers in `trigger_links`.
    pub fn with_sidecar(links: L, trigger_links: NamedTypesDecorator, trace: bool) -> Self {
        Self::new(links, TriggerStore::Sidecar(Box::new(trigger_links)), trace)
    }

    /// Whether replayed triggers may create missing references as points.
    pub fn with_auto_create_missing_references(mut self, enabled: bool) -> Self {
        self.auto_create_missing_references = enabled;
        self
    }

    pub fn auto_create_missing_references(&self) -> bool {
        self.auto_create_missing_references
    }

    pub fn set_auto_create_missing_references(&mut self, enabled: bool) {
        self.auto_create_missing_references = enabled;
    }

    pub fn inner(&self) -> &L {
        &self.links
    }

    pub fn inner_mut(&mut self) -> &mut L {
        &mut self.links
    }

    pub fn trigger_store(&self) -> &TriggerStore {
        &self.triggers
    }

    pub fn trigger_store_mut(&mut self) -> &mut TriggerStore {
        &mut self.triggers
    }

    /// Gives the decorated links and the trigger store back.
    pub fn into_parts(self) -> (L, TriggerStore) {
        (self.links, self.triggers)
    }

    /// Stores `query` as a trigger of `kind` and returns its root address.
    pub fn store_trigger(
        &mut self,
        kind: PersistentTransformationKind,
        query: &str,
    ) -> Result<u32> {
        let parsed = PersistentTransformationQuery::parse(query)?;
        let root = self.without_trigger_application(|this| {
            on_trigger_links!(this, store_trigger_in(kind, &parsed))
        })?;
        self.trace_msg(&format!(
            "Stored {kind} trigger #{root}: {}",
            parsed.query()
        ));
        Ok(root)
    }

    /// Removes every stored trigger whose query equals `query`, and returns how
    /// many were removed.
    pub fn remove_triggers(&mut self, query: &str) -> Result<usize> {
        let parsed = PersistentTransformationQuery::parse(query)?;
        self.without_trigger_application(|this| {
            let matching: Vec<u32> = this
                .triggers()?
                .into_iter()
                .filter(|trigger| {
                    trigger.condition == parsed.condition
                        && trigger.substitution == parsed.substitution
                })
                .map(|trigger| trigger.root)
                .collect();

            for root in &matching {
                on_trigger_links!(this, delete_trigger_root(*root))?;
                this.trace_msg(&format!("Deleted trigger #{root}"));
            }

            Ok(matching.len())
        })
    }

    /// Every stored trigger, ordered by root address.
    pub fn triggers(&mut self) -> Result<Vec<PersistentTransformation>> {
        on_trigger_links!(self, triggers_in())
    }

    /// Runs `action` with trigger application suppressed, restoring the
    /// previous setting afterwards. This is what keeps trigger bookkeeping from
    /// triggering itself.
    fn without_trigger_application<R>(&mut self, action: impl FnOnce(&mut Self) -> R) -> R {
        let previous = self.suppress_triggers;
        self.suppress_triggers = true;
        let result = action(self);
        self.suppress_triggers = previous;
        result
    }

    /// Records a trigger failure raised by an infallible write.
    fn after_write(&mut self) {
        if let Err(error) = self.apply_triggers_after_operation() {
            if self.pending_error.is_none() {
                self.pending_error = Some(error);
            }
        }
    }

    /// Surfaces (and clears) a failure parked by [`after_write`].
    fn take_pending_error(&mut self) -> Result<()> {
        match self.pending_error.take() {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    fn apply_triggers_after_operation(&mut self) -> Result<()> {
        if self.suppress_triggers || self.applying_triggers {
            return Ok(());
        }

        let triggers = self.triggers()?;
        if triggers.is_empty() {
            return Ok(());
        }

        self.applying_triggers = true;
        let outcome = self.apply_triggers(&triggers);
        self.applying_triggers = false;
        outcome
    }

    fn apply_triggers(&mut self, triggers: &[PersistentTransformation]) -> Result<()> {
        let processor = QueryProcessor::new(self.trace)
            .with_auto_create_missing_references(self.auto_create_missing_references);

        for trigger in triggers {
            let changes = processor.process_query(self, &trigger.query())?;
            if changes.is_empty() || trigger.kind != PersistentTransformationKind::Once {
                continue;
            }

            let root = trigger.root;
            self.without_trigger_application(|this| {
                on_trigger_links!(this, delete_trigger_root(root))
            })?;
            self.trace_msg(&format!("Deleted trigger #{root}"));
        }

        Ok(())
    }

    fn trace_msg(&self, message: &str) {
        if self.trace {
            println!("[PersistentTransformation] {message}");
        }
    }
}

impl<L: NamedTypeLinks> NamedTypeLinks for PersistentTransformationDecorator<L> {
    fn create(&mut self, source: u32, target: u32) -> u32 {
        let index = self.links.create(source, target);
        self.after_write();
        index
    }

    fn ensure_created(&mut self, id: u32) -> u32 {
        let index = self.links.ensure_created(id);
        self.after_write();
        index
    }

    fn get_link(&mut self, id: u32) -> Option<Link> {
        self.links.get_link(id)
    }

    fn exists(&mut self, id: u32) -> bool {
        self.links.exists(id)
    }

    fn update(&mut self, id: u32, source: u32, target: u32) -> Result<Link> {
        let link = self.links.update(id, source, target)?;
        self.apply_triggers_after_operation()?;
        self.take_pending_error()?;
        Ok(link)
    }

    fn delete(&mut self, id: u32) -> Result<Link> {
        let link = self.links.delete(id)?;
        self.apply_triggers_after_operation()?;
        self.take_pending_error()?;
        Ok(link)
    }

    fn all_links(&mut self) -> Vec<Link> {
        self.links.all_links()
    }

    fn search(&mut self, source: u32, target: u32) -> Option<u32> {
        self.links.search(source, target)
    }

    fn get_or_create(&mut self, source: u32, target: u32) -> u32 {
        let index = self.links.get_or_create(source, target);
        self.after_write();
        index
    }

    fn get_name(&mut self, id: u32) -> Result<Option<String>> {
        self.links.get_name(id)
    }

    fn set_name(&mut self, id: u32, name: &str) -> Result<u32> {
        self.links.set_name(id, name)
    }

    fn get_by_name(&mut self, name: &str) -> Result<Option<u32>> {
        self.links.get_by_name(name)
    }

    fn remove_name(&mut self, id: u32) -> Result<()> {
        self.links.remove_name(id)
    }

    fn save(&mut self) -> Result<()> {
        self.take_pending_error()?;
        self.links.save()?;
        if let TriggerStore::Sidecar(store) = &mut self.triggers {
            store.save()?;
        }
        Ok(())
    }
}
