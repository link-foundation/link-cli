//! Checked request execution shared by the generic and session servers.
use super::*;

/// Executes one request against `storage`.
///
/// A non-empty document is a substitution query; the reply holds one
/// `(before) (after)` line per change, exactly like `clink --changes`. The
/// empty document asks for every link, one `(index: source target)` per line.
/// Failures produce an [`error_document`].
pub fn execute_request<S: NamedTypeLinks>(
    storage: &mut S,
    processor: &QueryProcessor,
    document: &[LiNo<String>],
) -> LinoDocument {
    execute_observed_request(storage, processor, document, &mut Vec::new())
}

/// Executes a u32 request and records its net changes for subscriptions.
pub(crate) fn execute_observed_request<S: NamedTypeLinks>(
    storage: &mut S,
    processor: &QueryProcessor,
    document: &[LiNo<String>],
    changed: &mut Vec<Change>,
) -> LinoDocument {
    match try_execute_request(storage, processor, document, changed) {
        Ok(reply) => reply,
        Err(error) => error_document(&format!("{error:#}")),
    }
}

/// Executes an owned protocol document with the selected address type.
/// Checked storage policies admit the document before recursive formatting.
pub fn execute_request_for<T: QueryAddress, S: NamedTypeLinks<T>>(
    storage: &mut S,
    processor: &GenericQueryProcessor<T>,
    document: &[LiNo<String>],
) -> LinoDocument {
    match try_execute_request(storage, processor, document, &mut Vec::new()) {
        Ok(reply) => reply,
        Err(error) => error_document(&format!("{error:#}")),
    }
}

fn try_execute_request<T: QueryAddress, S: NamedTypeLinks<T>>(
    storage: &mut S,
    processor: &GenericQueryProcessor<T>,
    document: &[LiNo<String>],
    changed: &mut Vec<Change<T>>,
) -> anyhow::Result<LinoDocument> {
    if let Some(limits) = storage.query_limits() {
        admit_document(document, limits)?;
    }
    // A canonical document does not retain quoted-reference kinds. The envelope
    // carries original query text as one value through both text and binary.
    let original = match document {
        [LiNo::Link {
            id: Some(id),
            values,
        }] if id == "query" => {
            let [LiNo::Ref(text)] = values.as_slice() else {
                anyhow::bail!("expected (query: original-query-text)");
            };
            Some(text.as_str())
        }
        _ => None,
    };
    if original.is_none() {
        if let Some(operation) = GenericLinksOperation::<T>::from_document(document)? {
            let reply = operation.execute(storage)?;
            if matches!(
                operation,
                GenericLinksOperation::Create { .. }
                    | GenericLinksOperation::Update { .. }
                    | GenericLinksOperation::Delete(_)
            ) {
                changed.extend(parse_changes_for::<T>(&reply)?);
            }
            return Ok(reply);
        }
    }
    if document.is_empty() || original.is_some_and(|text| text.trim().is_empty()) {
        let mut budget = QueryReplyBudget::new(storage.query_limits());
        let mut links = storage.all_links();
        if let Some(limits) = storage.query_limits() {
            anyhow::ensure!(
                links.len() <= limits.max_results,
                "protocol result limit exceeded"
            );
        }
        links.sort_by_key(|link| link.index);
        return links
            .iter()
            .map(|link| link_lino(storage, link, &mut budget))
            .collect();
    }
    let legacy_text;
    let text = match original {
        Some(text) => text,
        None => {
            legacy_text = format_document(document);
            &legacy_text
        }
    };
    let changes = processor.process_query(storage, text)?;
    changed.extend(changes.iter().map(|(before, after)| {
        (
            before.unwrap_or_else(GenericLink::null),
            after.unwrap_or_else(GenericLink::null),
        )
    }));
    if !changes.is_empty() {
        storage.save()?;
    }
    let mut budget = QueryReplyBudget::new(storage.query_limits());
    changes
        .iter()
        .map(|(before, after)| {
            Ok(LiNo::Link {
                id: None,
                values: vec![
                    change_side(storage, before.as_ref(), &mut budget)?,
                    change_side(storage, after.as_ref(), &mut budget)?,
                ],
            })
        })
        .collect()
}

/// `(index: source target)`, naming every reference that has a name.
fn link_lino<T: QueryAddress, S: NamedTypeLinks<T> + ?Sized>(
    storage: &mut S,
    link: &GenericLink<T>,
    budget: &mut QueryReplyBudget,
) -> anyhow::Result<LiNo<String>> {
    Ok(LiNo::Link {
        id: Some(budget.reference(storage, link.index)?),
        values: vec![
            LiNo::Ref(budget.reference(storage, link.source)?),
            LiNo::Ref(budget.reference(storage, link.target)?),
        ],
    })
}

/// `()` for a missing side of a change, `((index: source target))` otherwise.
fn change_side<T: QueryAddress, S: NamedTypeLinks<T> + ?Sized>(
    storage: &mut S,
    link: Option<&GenericLink<T>>,
    budget: &mut QueryReplyBudget,
) -> anyhow::Result<LiNo<String>> {
    let values = match link {
        Some(link) => vec![link_lino(storage, link, budget)?],
        None => Vec::new(),
    };
    Ok(LiNo::Link { id: None, values })
}

/// Carries the exact original query text, including quoted-reference kinds.
/// Older peers must explicitly support this envelope before a client uses it.
pub fn query_document(text: &str) -> LinoDocument {
    vec![LiNo::Link {
        id: Some("query".into()),
        values: vec![LiNo::Ref(text.into())],
    }]
}

pub(crate) fn admit_document(document: &[LiNo<String>], limits: QueryLimits) -> anyhow::Result<()> {
    anyhow::ensure!(
        document.len() <= limits.max_nodes,
        "protocol node limit exceeded"
    );
    let mut pending: Vec<_> = document.iter().map(|node| (node, 0usize)).collect();
    let mut admitted = document.len();
    let mut bytes = 0usize;
    while let Some((node, depth)) = pending.pop() {
        anyhow::ensure!(depth <= limits.max_depth, "protocol depth limit exceeded");
        match node {
            LiNo::Ref(value) => bytes = bytes.saturating_add(value.len()),
            LiNo::Link { id, values } => {
                bytes = bytes.saturating_add(id.as_ref().map_or(0, String::len));
                admitted = admitted.saturating_add(values.len());
                anyhow::ensure!(admitted <= limits.max_nodes, "protocol node limit exceeded");
                pending.extend(values.iter().map(|node| (node, depth + 1)));
            }
        }
        anyhow::ensure!(bytes <= limits.max_bytes, "protocol byte limit exceeded");
    }
    Ok(())
}

/// Parses bounded transport text before converting or formatting its document.
/// The upstream grammar enforces depth before building its recursive model.
pub fn parse_document_with_limits(text: &str, limits: QueryLimits) -> ProtocolResult<LinoDocument> {
    use links_notation::{parse_lino_to_links_with_config, ParserConfig};
    if text.len() > limits.max_bytes {
        return Err(crate::protocol::error::ProtocolError::LimitExceeded(
            "protocol byte limit exceeded".into(),
        ));
    }
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    let document = parse_lino_to_links_with_config(
        text,
        &ParserConfig::new().with_max_depth(limits.max_depth),
    )
    .map_err(|error| crate::protocol::error::ProtocolError::InvalidLino(error.to_string()))?;
    admit_document(&document, limits)
        .map_err(|error| crate::protocol::error::ProtocolError::LimitExceeded(error.to_string()))?;
    Ok(document
        .into_iter()
        .map(links_notation::binary::canonical)
        .collect())
}

pub(crate) fn reference_name<S: NamedTypeLinks + ?Sized>(
    storage: &mut S,
    id: u32,
) -> anyhow::Result<String> {
    Ok(storage.get_name(id)?.unwrap_or_else(|| id.to_string()))
}
