//! GraphQL documents: operations, selections, fragments and values.

use std::collections::HashMap;

use anyhow::{anyhow, bail, Result};

use super::json::Json;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Variable(String),
    Literal(Json),
    List(Vec<Value>),
    Object(Vec<(String, Value)>),
}

impl Value {
    /// The value with its variables replaced; a variable nobody passed is null.
    pub fn resolve(&self, variables: &Json) -> Json {
        match self {
            Value::Variable(name) => variables.get(name).cloned().unwrap_or(Json::Null),
            Value::Literal(json) => json.clone(),
            Value::List(items) => {
                Json::Array(items.iter().map(|item| item.resolve(variables)).collect())
            }
            Value::Object(pairs) => Json::Object(
                pairs
                    .iter()
                    .map(|(key, value)| (key.clone(), value.resolve(variables)))
                    .collect(),
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Directive {
    pub name: String,
    pub arguments: Vec<(String, Value)>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Selection {
    Field(Field),
    Spread(String, Vec<Directive>),
    Inline(Option<String>, Vec<Directive>, Vec<Selection>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub alias: Option<String>,
    pub name: String,
    pub arguments: Vec<(String, Value)>,
    pub directives: Vec<Directive>,
    pub selections: Vec<Selection>,
}

impl Field {
    pub fn response_key(&self) -> &str {
        self.alias.as_deref().unwrap_or(&self.name)
    }

    pub fn argument(&self, name: &str, variables: &Json) -> Option<Json> {
        self.arguments
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.resolve(variables))
            .filter(|value| !value.is_null())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationKind {
    Query,
    Mutation,
    Subscription,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Operation {
    pub kind: OperationKind,
    pub name: Option<String>,
    /// Variable name and default value.
    pub variables: Vec<(String, Option<Value>)>,
    pub selections: Vec<Selection>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Document {
    pub operations: Vec<Operation>,
    pub fragments: HashMap<String, Vec<Selection>>,
}

impl Document {
    pub fn parse(text: &str) -> Result<Document> {
        let tokens = tokenize(text)?;
        let mut parser = Parser { tokens, at: 0 };
        let mut document = Document::default();
        while parser.peek().is_some() {
            if parser.eat_name("fragment") {
                let name = parser.name()?;
                if !parser.eat_name("on") {
                    bail!("Expected 'on' in fragment {name}.");
                }
                parser.name()?;
                parser.directives(0)?;
                let selections = parser.selection_set(0)?;
                document.fragments.insert(name, selections);
            } else {
                document.operations.push(parser.operation()?);
            }
        }
        if document.operations.is_empty() {
            bail!("A document has at least one operation.");
        }
        Ok(document)
    }

    pub fn operation(&self, name: Option<&str>) -> Result<&Operation> {
        match name {
            Some(name) => self
                .operations
                .iter()
                .find(|operation| operation.name.as_deref() == Some(name))
                .ok_or_else(|| anyhow!("Unknown operation named '{name}'.")),
            None if self.operations.len() == 1 => Ok(&self.operations[0]),
            None => bail!("Must provide operation name if query contains multiple operations."),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Punctuator(char),
    Name(String),
    Int(i64),
    Float(f64),
    String(String),
}

fn tokenize(text: &str) -> Result<Vec<Token>> {
    let chars: Vec<char> = text.chars().collect();
    let mut tokens = Vec::new();
    let mut at = 0;
    while at < chars.len() {
        let character = chars[at];
        match character {
            ' ' | '\n' | '\r' | '\t' | ',' | '\u{feff}' => at += 1,
            '#' => {
                while at < chars.len() && chars[at] != '\n' {
                    at += 1;
                }
            }
            '!' | '$' | '(' | ')' | ':' | '=' | '@' | '[' | ']' | '{' | '}' | '|' | '&' => {
                tokens.push(Token::Punctuator(character));
                at += 1;
            }
            '.' => {
                if chars.get(at + 1) == Some(&'.') && chars.get(at + 2) == Some(&'.') {
                    tokens.push(Token::Punctuator('.'));
                    at += 3;
                } else {
                    bail!("Unexpected '.' in the document.");
                }
            }
            '"' => {
                if chars.get(at + 1) == Some(&'"') && chars.get(at + 2) == Some(&'"') {
                    at += 3;
                    let mut out = String::new();
                    loop {
                        if at + 2 >= chars.len() {
                            bail!("Unterminated block string.");
                        }
                        if chars[at] == '"' && chars[at + 1] == '"' && chars[at + 2] == '"' {
                            at += 3;
                            break;
                        }
                        out.push(chars[at]);
                        at += 1;
                    }
                    tokens.push(Token::String(out));
                } else {
                    let start = at;
                    at += 1;
                    while at < chars.len() && chars[at] != '"' {
                        at += if chars[at] == '\\' { 2 } else { 1 };
                    }
                    if at >= chars.len() {
                        bail!("Unterminated string.");
                    }
                    at += 1;
                    let literal: String = chars[start..at].iter().collect();
                    match Json::parse(&literal)? {
                        Json::String(value) => tokens.push(Token::String(value)),
                        _ => bail!("Invalid string."),
                    }
                }
            }
            '-' | '0'..='9' => {
                let start = at;
                at += 1;
                while at < chars.len()
                    && matches!(chars[at], '0'..='9' | '.' | 'e' | 'E' | '+' | '-')
                {
                    at += 1;
                }
                let literal: String = chars[start..at].iter().collect();
                if let Ok(value) = literal.parse::<i64>() {
                    tokens.push(Token::Int(value));
                } else if let Ok(value) = literal.parse::<f64>() {
                    tokens.push(Token::Float(value));
                } else {
                    bail!("Invalid number '{literal}'.");
                }
            }
            '_' | 'a'..='z' | 'A'..='Z' => {
                let start = at;
                while at < chars.len() && (chars[at] == '_' || chars[at].is_ascii_alphanumeric()) {
                    at += 1;
                }
                tokens.push(Token::Name(chars[start..at].iter().collect()));
            }
            other => bail!("Unexpected character '{other}' in the document."),
        }
    }
    Ok(tokens)
}

/// Deep enough for any selection a client means.
const MAX_DEPTH: usize = 64;

struct Parser {
    tokens: Vec<Token>,
    at: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.at)
    }

    fn eat(&mut self, punctuator: char) -> bool {
        if self.peek() == Some(&Token::Punctuator(punctuator)) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, punctuator: char) -> Result<()> {
        if self.eat(punctuator) {
            Ok(())
        } else {
            bail!("Expected '{punctuator}', found {:?}.", self.peek())
        }
    }

    fn eat_name(&mut self, name: &str) -> bool {
        if matches!(self.peek(), Some(Token::Name(found)) if found == name) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn name(&mut self) -> Result<String> {
        match self.peek() {
            Some(Token::Name(name)) => {
                let name = name.clone();
                self.at += 1;
                Ok(name)
            }
            other => bail!("Expected a name, found {other:?}."),
        }
    }

    fn operation(&mut self) -> Result<Operation> {
        if self.peek() == Some(&Token::Punctuator('{')) {
            return Ok(Operation {
                kind: OperationKind::Query,
                name: None,
                variables: Vec::new(),
                selections: self.selection_set(0)?,
            });
        }
        let keyword = self.name()?;
        let kind = match keyword.as_str() {
            "query" => OperationKind::Query,
            "mutation" => OperationKind::Mutation,
            "subscription" => OperationKind::Subscription,
            other => bail!("Unknown operation type '{other}'."),
        };
        let name = match self.peek() {
            Some(Token::Name(_)) => Some(self.name()?),
            _ => None,
        };
        let mut variables = Vec::new();
        if self.eat('(') {
            while !self.eat(')') {
                self.expect('$')?;
                let variable = self.name()?;
                self.expect(':')?;
                self.skip_type(0)?;
                let default = if self.eat('=') {
                    Some(self.value(0)?)
                } else {
                    None
                };
                self.directives(0)?;
                variables.push((variable, default));
            }
        }
        self.directives(0)?;
        Ok(Operation {
            kind,
            name,
            variables,
            selections: self.selection_set(0)?,
        })
    }

    fn skip_type(&mut self, depth: usize) -> Result<()> {
        if depth > MAX_DEPTH {
            bail!("The type is nested too deeply.");
        }
        if self.eat('[') {
            self.skip_type(depth + 1)?;
            self.expect(']')?;
        } else {
            self.name()?;
        }
        self.eat('!');
        Ok(())
    }

    fn selection_set(&mut self, depth: usize) -> Result<Vec<Selection>> {
        if depth > MAX_DEPTH {
            bail!("The selection is nested too deeply.");
        }
        self.expect('{')?;
        let mut selections = Vec::new();
        while !self.eat('}') {
            if self.eat('.') {
                if self.eat_name("on") {
                    let on = self.name()?;
                    let directives = self.directives(depth)?;
                    selections.push(Selection::Inline(
                        Some(on),
                        directives,
                        self.selection_set(depth + 1)?,
                    ));
                } else if matches!(self.peek(), Some(Token::Name(_))) {
                    let name = self.name()?;
                    let directives = self.directives(depth)?;
                    selections.push(Selection::Spread(name, directives));
                } else {
                    let directives = self.directives(depth)?;
                    selections.push(Selection::Inline(
                        None,
                        directives,
                        self.selection_set(depth + 1)?,
                    ));
                }
            } else {
                let first = self.name()?;
                let (alias, name) = if self.eat(':') {
                    (Some(first), self.name()?)
                } else {
                    (None, first)
                };
                let arguments = self.arguments(depth)?;
                let directives = self.directives(depth)?;
                let selections_of = if self.peek() == Some(&Token::Punctuator('{')) {
                    self.selection_set(depth + 1)?
                } else {
                    Vec::new()
                };
                selections.push(Selection::Field(Field {
                    alias,
                    name,
                    arguments,
                    directives,
                    selections: selections_of,
                }));
            }
        }
        Ok(selections)
    }

    fn arguments(&mut self, depth: usize) -> Result<Vec<(String, Value)>> {
        let mut arguments = Vec::new();
        if self.eat('(') {
            while !self.eat(')') {
                let name = self.name()?;
                self.expect(':')?;
                arguments.push((name, self.value(depth)?));
            }
        }
        Ok(arguments)
    }

    fn directives(&mut self, depth: usize) -> Result<Vec<Directive>> {
        let mut directives = Vec::new();
        while self.eat('@') {
            let name = self.name()?;
            let arguments = self.arguments(depth)?;
            directives.push(Directive { name, arguments });
        }
        Ok(directives)
    }

    fn value(&mut self, depth: usize) -> Result<Value> {
        if depth > MAX_DEPTH {
            bail!("The value is nested too deeply.");
        }
        let token = self.peek().cloned();
        self.at += 1;
        Ok(match token {
            Some(Token::Punctuator('$')) => Value::Variable(self.name()?),
            Some(Token::Int(value)) => Value::Literal(Json::Int(value)),
            Some(Token::Float(value)) => Value::Literal(Json::Float(value)),
            Some(Token::String(value)) => Value::Literal(Json::String(value)),
            Some(Token::Name(name)) => Value::Literal(match name.as_str() {
                "true" => Json::Bool(true),
                "false" => Json::Bool(false),
                "null" => Json::Null,
                // An enum value travels as its name, as JSON variables do.
                _ => Json::String(name),
            }),
            Some(Token::Punctuator('[')) => {
                let mut items = Vec::new();
                while !self.eat(']') {
                    items.push(self.value(depth + 1)?);
                }
                Value::List(items)
            }
            Some(Token::Punctuator('{')) => {
                let mut pairs = Vec::new();
                while !self.eat('}') {
                    let name = self.name()?;
                    self.expect(':')?;
                    pairs.push((name, self.value(depth + 1)?));
                }
                Value::Object(pairs)
            }
            other => bail!("Expected a value, found {other:?}."),
        })
    }
}
