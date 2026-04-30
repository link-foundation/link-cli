use std::collections::HashMap;
use std::hash::Hash;

#[derive(Clone, Debug)]
pub struct CachingConverterDecorator<K, V> {
    cache: HashMap<K, V>,
}

impl<K, V> Default for CachingConverterDecorator<K, V> {
    fn default() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }
}

impl<K, V> CachingConverterDecorator<K, V>
where
    K: Eq + Hash + Clone,
    V: Clone,
{
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, input: &K) -> Option<V> {
        self.cache.get(input).cloned()
    }

    pub fn insert(&mut self, input: K, output: V) -> V {
        self.cache.insert(input, output.clone());
        output
    }

    pub fn convert_with<F, E>(&mut self, input: K, convert: F) -> Result<V, E>
    where
        F: FnOnce(&K) -> Result<V, E>,
    {
        if let Some(output) = self.get(&input) {
            return Ok(output);
        }
        let output = convert(&input)?;
        Ok(self.insert(input, output))
    }

    pub fn len(&self) -> usize {
        self.cache.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }
}
