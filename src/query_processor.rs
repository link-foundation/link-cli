use crate::{ClinkOptions, links_operations::LinksStorage};

pub struct QueryProcessor {
    links: LinksStorage,
}

impl QueryProcessor {
    pub fn new() -> Self {
        Self {
            links: LinksStorage::new(),
        }
    }

    pub fn execute(&mut self, query: &str, options: &ClinkOptions) -> Result<String, String> {
        let mut output = Vec::new();
        
        if options.trace {
            output.push(format!("Executing query: {}", query));
        }

        if options.before {
            let before_state = self.links.get_all_links();
            if !before_state.is_empty() {
                for link in &before_state {
                    output.push(format!("({}: {} {})", link.id, link.source, link.target));
                }
            }
        }

        // Parse and execute the query
        let changes = self.parse_and_execute_query(query)?;

        if options.changes {
            for change in &changes {
                output.push(change.clone());
            }
        }

        if options.after {
            let after_state = self.links.get_all_links();
            for link in &after_state {
                output.push(format!("({}: {} {})", link.id, link.source, link.target));
            }
        }

        Ok(output.join("\n"))
    }

    fn parse_and_execute_query(&mut self, query: &str) -> Result<Vec<String>, String> {
        // Simplified LiNo parser - this would need to be more robust in a real implementation
        let query = query.trim();
        
        if query.is_empty() {
            return Ok(vec![]);
        }

        // Handle basic patterns like: () ((1 1)) for creation
        if query.starts_with("() ((") && query.ends_with("))") {
            return self.handle_creation(query);
        }

        // Handle deletion patterns like: ((1 1)) ()
        if query.ends_with(" ()") && query.starts_with("((") {
            return self.handle_deletion(query);
        }

        // Handle update patterns like: ((1: 1 1)) ((1: 1 2))
        if query.contains(")) ((") {
            return self.handle_update(query);
        }

        // Handle read patterns like: ((($i: $s $t)) (($i: $s $t)))
        if query.contains("$i") || query.contains("$s") || query.contains("$t") {
            return self.handle_read(query);
        }

        Err("Unsupported query format".to_string())
    }

    fn handle_creation(&mut self, query: &str) -> Result<Vec<String>, String> {
        // Extract links to create from pattern like: () ((1 1) (2 2))
        let links_part = query.strip_prefix("() ((").and_then(|s| s.strip_suffix("))"));
        
        if let Some(links_str) = links_part {
            let mut changes = Vec::new();
            
            // Parse individual links
            for link_str in self.parse_links(links_str) {
                let (source, target) = self.parse_link_pair(&link_str)?;
                let new_id = self.links.create_link(source, target);
                changes.push(format!("() (({}: {} {}))", new_id, source, target));
            }
            
            Ok(changes)
        } else {
            Err("Invalid creation query format".to_string())
        }
    }

    fn handle_deletion(&mut self, query: &str) -> Result<Vec<String>, String> {
        // Extract links to delete from pattern like: ((1 1)) ()
        let links_part = query.strip_suffix(" ()").and_then(|s| s.strip_prefix("((")).and_then(|s| s.strip_suffix("))"));
        
        if let Some(links_str) = links_part {
            let mut changes = Vec::new();
            
            for link_str in self.parse_links(links_str) {
                if link_str.contains("*") {
                    // Delete all links
                    let all_links = self.links.get_all_links();
                    for link in all_links {
                        self.links.delete_link(link.id);
                        changes.push(format!("(({}: {} {})) ()", link.id, link.source, link.target));
                    }
                } else {
                    let (source, target) = self.parse_link_pair(&link_str)?;
                    if let Some(link) = self.links.find_link(source, target) {
                        let link_id = link.id;
                        self.links.delete_link(link_id);
                        changes.push(format!("(({}: {} {})) ()", link_id, source, target));
                    }
                }
            }
            
            Ok(changes)
        } else {
            Err("Invalid deletion query format".to_string())
        }
    }

    fn handle_update(&mut self, query: &str) -> Result<Vec<String>, String> {
        // Parse update pattern like: ((1: 1 1)) ((1: 1 2))
        let parts: Vec<&str> = query.split(")) ((").collect();
        if parts.len() != 2 {
            return Err("Invalid update query format".to_string());
        }

        let from_part = parts[0].strip_prefix("((").unwrap_or(parts[0]);
        let to_part = parts[1].strip_suffix("))").unwrap_or(parts[1]);

        let mut changes = Vec::new();

        // Parse from and to links
        for (from_link, to_link) in self.parse_links(from_part).into_iter().zip(self.parse_links(to_part)) {
            let (from_id, from_source, from_target) = self.parse_link_with_id(&from_link)?;
            let (to_id, to_source, to_target) = self.parse_link_with_id(&to_link)?;

            if from_id == to_id {
                self.links.update_link(from_id, to_source, to_target);
                changes.push(format!("(({}: {} {})) (({}: {} {}))", from_id, from_source, from_target, to_id, to_source, to_target));
            }
        }

        Ok(changes)
    }

    fn handle_read(&mut self, _query: &str) -> Result<Vec<String>, String> {
        // Handle read queries with variables
        let mut changes = Vec::new();
        let all_links = self.links.get_all_links();
        
        for link in &all_links {
            changes.push(format!("(({}: {} {})) (({}: {} {}))", link.id, link.source, link.target, link.id, link.source, link.target));
        }
        
        Ok(changes)
    }

    fn parse_links(&self, links_str: &str) -> Vec<String> {
        // Simple parser for extracting individual links from a string like "(1 1) (2 2)"
        let mut links = Vec::new();
        let mut current_link = String::new();
        let mut paren_count = 0;

        for ch in links_str.chars() {
            match ch {
                '(' => {
                    paren_count += 1;
                    current_link.push(ch);
                }
                ')' => {
                    paren_count -= 1;
                    current_link.push(ch);
                    if paren_count == 0 && !current_link.trim().is_empty() {
                        links.push(current_link.trim().to_string());
                        current_link.clear();
                    }
                }
                ' ' if paren_count == 0 => {
                    // Skip spaces between links
                }
                _ => {
                    current_link.push(ch);
                }
            }
        }

        if !current_link.trim().is_empty() {
            links.push(current_link.trim().to_string());
        }

        links
    }

    fn parse_link_pair(&self, link_str: &str) -> Result<(u64, u64), String> {
        // Parse a link like "(1 1)" or "1 1"
        let link_str = link_str.trim_start_matches('(').trim_end_matches(')');
        let parts: Vec<&str> = link_str.split_whitespace().collect();
        
        if parts.len() >= 2 {
            let source = parts[0].parse::<u64>().map_err(|_| "Invalid source ID")?;
            let target = parts[1].parse::<u64>().map_err(|_| "Invalid target ID")?;
            Ok((source, target))
        } else {
            Err("Invalid link format".to_string())
        }
    }

    fn parse_link_with_id(&self, link_str: &str) -> Result<(u64, u64, u64), String> {
        // Parse a link like "(1: 1 1)" or "1: 1 1"
        let link_str = link_str.trim_start_matches('(').trim_end_matches(')');
        let parts: Vec<&str> = link_str.split_whitespace().collect();
        
        if parts.len() >= 3 {
            let id_part = parts[0].trim_end_matches(':');
            let id = id_part.parse::<u64>().map_err(|_| "Invalid link ID")?;
            let source = parts[1].parse::<u64>().map_err(|_| "Invalid source ID")?;
            let target = parts[2].parse::<u64>().map_err(|_| "Invalid target ID")?;
            Ok((id, source, target))
        } else {
            Err("Invalid link format with ID".to_string())
        }
    }
}