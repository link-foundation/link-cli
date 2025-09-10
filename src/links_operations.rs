use std::collections::HashMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Link {
    pub id: u64,
    pub source: u64,
    pub target: u64,
}

pub struct LinksStorage {
    links: HashMap<u64, Link>,
    next_id: u64,
}

impl LinksStorage {
    pub fn new() -> Self {
        Self {
            links: HashMap::new(),
            next_id: 1,
        }
    }

    pub fn create_link(&mut self, source: u64, target: u64) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        
        let link = Link { id, source, target };
        self.links.insert(id, link);
        
        id
    }

    pub fn update_link(&mut self, id: u64, source: u64, target: u64) -> bool {
        if let Some(link) = self.links.get_mut(&id) {
            link.source = source;
            link.target = target;
            true
        } else {
            false
        }
    }

    pub fn delete_link(&mut self, id: u64) -> bool {
        self.links.remove(&id).is_some()
    }

    pub fn get_link(&self, id: u64) -> Option<&Link> {
        self.links.get(&id)
    }

    pub fn find_link(&self, source: u64, target: u64) -> Option<&Link> {
        self.links.values().find(|link| link.source == source && link.target == target)
    }

    pub fn get_all_links(&self) -> Vec<Link> {
        let mut links: Vec<Link> = self.links.values().cloned().collect();
        links.sort_by_key(|link| link.id);
        links
    }

    pub fn clear(&mut self) {
        self.links.clear();
        self.next_id = 1;
    }

    pub fn count(&self) -> usize {
        self.links.len()
    }

    pub fn exists(&self, id: u64) -> bool {
        self.links.contains_key(&id)
    }

    pub fn get_links_by_source(&self, source: u64) -> Vec<&Link> {
        self.links.values().filter(|link| link.source == source).collect()
    }

    pub fn get_links_by_target(&self, target: u64) -> Vec<&Link> {
        self.links.values().filter(|link| link.target == target).collect()
    }

    pub fn get_links_by_source_and_target(&self, source: u64, target: u64) -> Vec<&Link> {
        self.links.values()
            .filter(|link| link.source == source && link.target == target)
            .collect()
    }
}