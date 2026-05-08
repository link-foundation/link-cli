use crate::link_storage::LinkStorage;

#[derive(Clone, Copy, Debug, Default)]
pub struct BalancedVariantConverter;

impl BalancedVariantConverter {
    pub fn new() -> Self {
        Self
    }

    pub fn convert(&self, links: &mut LinkStorage, elements: &[u32]) -> u32 {
        match elements.len() {
            0 => 0,
            1 => elements[0],
            2 => links.get_or_create(elements[0], elements[1]),
            _ => {
                let mut layer = elements.to_vec();
                while layer.len() > 2 {
                    let mut next = Vec::with_capacity(layer.len().div_ceil(2));
                    let mut chunks = layer.chunks_exact(2);
                    for pair in &mut chunks {
                        next.push(links.get_or_create(pair[0], pair[1]));
                    }
                    if let Some(&remainder) = chunks.remainder().first() {
                        next.push(remainder);
                    }
                    layer = next;
                }
                links.get_or_create(layer[0], layer[1])
            }
        }
    }
}
