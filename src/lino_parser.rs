use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LiNo {
    Link {
        id: Option<u64>,
        source: Option<Box<LiNo>>,
        target: Option<Box<LiNo>>,
    },
    Ref(u64),
}

impl LiNo {
    pub fn is_ref(&self) -> bool {
        matches!(self, LiNo::Ref(_))
    }

    pub fn is_link(&self) -> bool {
        matches!(self, LiNo::Link { .. })
    }

    pub fn get_ref_value(&self) -> Option<u64> {
        match self {
            LiNo::Ref(value) => Some(*value),
            _ => None,
        }
    }
}

impl fmt::Display for LiNo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LiNo::Ref(value) => write!(f, "{}", value),
            LiNo::Link { id, source, target } => {
                write!(f, "(")?;
                
                if let Some(id_val) = id {
                    write!(f, "{}: ", id_val)?;
                }
                
                if let Some(src) = source {
                    write!(f, "{}", src)?;
                }
                
                if source.is_some() && target.is_some() {
                    write!(f, " ")?;
                }
                
                if let Some(tgt) = target {
                    write!(f, "{}", tgt)?;
                }
                
                write!(f, ")")
            }
        }
    }
}

pub struct LiNoParser;

impl LiNoParser {
    pub fn new() -> Self {
        Self
    }

    pub fn parse(&self, input: &str) -> Result<LiNo, String> {
        let input = input.trim();
        
        if input.is_empty() {
            return Err("Empty input".to_string());
        }

        // Parse as reference (simple number)
        if let Ok(num) = input.parse::<u64>() {
            return Ok(LiNo::Ref(num));
        }

        // Parse as link (parentheses)
        if input.starts_with('(') && input.ends_with(')') {
            return self.parse_link(input);
        }

        Err(format!("Invalid LiNo format: {}", input))
    }

    fn parse_link(&self, input: &str) -> Result<LiNo, String> {
        let content = &input[1..input.len()-1]; // Remove outer parentheses
        let content = content.trim();
        
        if content.is_empty() {
            return Ok(LiNo::Link {
                id: None,
                source: None,
                target: None,
            });
        }

        // Check if it has an ID (contains colon)
        if content.contains(':') {
            self.parse_link_with_id(content)
        } else {
            self.parse_link_without_id(content)
        }
    }

    fn parse_link_with_id(&self, content: &str) -> Result<LiNo, String> {
        let parts: Vec<&str> = content.splitn(2, ':').collect();
        if parts.len() != 2 {
            return Err("Invalid link format with ID".to_string());
        }

        let id = parts[0].trim().parse::<u64>().map_err(|_| "Invalid ID")?;
        let rest = parts[1].trim();
        
        let (source, target) = self.parse_source_target(rest)?;
        
        Ok(LiNo::Link {
            id: Some(id),
            source,
            target,
        })
    }

    fn parse_link_without_id(&self, content: &str) -> Result<LiNo, String> {
        let (source, target) = self.parse_source_target(content)?;
        
        Ok(LiNo::Link {
            id: None,
            source,
            target,
        })
    }

    fn parse_source_target(&self, content: &str) -> Result<(Option<Box<LiNo>>, Option<Box<LiNo>>), String> {
        if content.is_empty() {
            return Ok((None, None));
        }

        // Handle variables (starts with $)
        if content.starts_with('$') {
            // Variables are treated as references for simplicity
            return Ok((None, None));
        }

        // Handle wildcards
        if content == "*" || content == "* *" {
            return Ok((None, None));
        }

        // Simple case: two numbers
        let tokens = self.tokenize(content);
        
        if tokens.len() == 0 {
            return Ok((None, None));
        } else if tokens.len() == 1 {
            let source = self.parse_token(&tokens[0])?;
            return Ok((Some(Box::new(source)), None));
        } else if tokens.len() == 2 {
            let source = self.parse_token(&tokens[0])?;
            let target = self.parse_token(&tokens[1])?;
            return Ok((Some(Box::new(source)), Some(Box::new(target))));
        } else {
            return Err("Too many elements in link".to_string());
        }
    }

    fn tokenize(&self, input: &str) -> Vec<String> {
        let mut tokens = Vec::new();
        let mut current_token = String::new();
        let mut paren_depth = 0;
        let mut in_token = false;

        for ch in input.chars() {
            match ch {
                '(' => {
                    paren_depth += 1;
                    current_token.push(ch);
                    in_token = true;
                }
                ')' => {
                    paren_depth -= 1;
                    current_token.push(ch);
                    if paren_depth == 0 && in_token {
                        tokens.push(current_token.trim().to_string());
                        current_token.clear();
                        in_token = false;
                    }
                }
                ' ' | '\t' | '\n' => {
                    if paren_depth > 0 {
                        current_token.push(ch);
                    } else if in_token && !current_token.trim().is_empty() {
                        tokens.push(current_token.trim().to_string());
                        current_token.clear();
                        in_token = false;
                    }
                }
                _ => {
                    current_token.push(ch);
                    in_token = true;
                }
            }
        }

        if !current_token.trim().is_empty() {
            tokens.push(current_token.trim().to_string());
        }

        tokens
    }

    fn parse_token(&self, token: &str) -> Result<LiNo, String> {
        let token = token.trim();
        
        // Skip variables and wildcards
        if token.starts_with('$') || token == "*" {
            return Ok(LiNo::Ref(0)); // Placeholder
        }

        // Try to parse as number first
        if let Ok(num) = token.parse::<u64>() {
            return Ok(LiNo::Ref(num));
        }

        // Try to parse as nested link
        if token.starts_with('(') && token.ends_with(')') {
            return self.parse_link(token);
        }

        Err(format!("Invalid token: {}", token))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_ref() {
        let parser = LiNoParser::new();
        let result = parser.parse("42").unwrap();
        assert_eq!(result, LiNo::Ref(42));
        assert!(result.is_ref());
        assert!(!result.is_link());
        assert_eq!(result.get_ref_value(), Some(42));
    }

    #[test]
    fn test_empty_link() {
        let parser = LiNoParser::new();
        let result = parser.parse("()").unwrap();
        assert!(result.is_link());
        assert!(!result.is_ref());
    }

    #[test]
    fn test_link_with_source_and_target() {
        let parser = LiNoParser::new();
        let result = parser.parse("(1 2)").unwrap();
        assert!(result.is_link());
        
        if let LiNo::Link { id, source, target } = result {
            assert_eq!(id, None);
            assert!(source.is_some());
            assert!(target.is_some());
        }
    }

    #[test]
    fn test_link_with_id() {
        let parser = LiNoParser::new();
        let result = parser.parse("(3: 1 2)").unwrap();
        assert!(result.is_link());
        
        if let LiNo::Link { id, source, target } = result {
            assert_eq!(id, Some(3));
            assert!(source.is_some());
            assert!(target.is_some());
        }
    }

    #[test]
    fn test_display() {
        let ref_lino = LiNo::Ref(42);
        assert_eq!(format!("{}", ref_lino), "42");

        let link_lino = LiNo::Link {
            id: Some(1),
            source: Some(Box::new(LiNo::Ref(2))),
            target: Some(Box::new(LiNo::Ref(3))),
        };
        assert_eq!(format!("{}", link_lino), "(1: 2 3)");
    }

    #[test]
    fn test_invalid_input() {
        let parser = LiNoParser::new();
        assert!(parser.parse("").is_err());
        assert!(parser.parse("invalid").is_err());
    }
}