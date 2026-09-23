//! HelixSQL lexer, parser, validator, and HelixDB v1 wire compiler.
//!
//! This module is the authoritative language implementation. The webview sends
//! source text across IPC and receives only a compiled preview or an executed
//! result; no AST or traversal-building logic lives in React.

use serde::Serialize;
use serde_json::{json, Map, Value};

pub const DEFAULT_ROW_LIMIT: u64 = 500;
pub const DEFAULT_GRAPH_NODE_LIMIT: u64 = 400;
pub const DEFAULT_GRAPH_EDGE_LIMIT: u64 = 2_000;

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Debug, Serialize, thiserror::Error)]
#[error("{message}")]
#[serde(rename_all = "camelCase")]
pub struct HqlError {
    pub message: String,
    pub span: Option<Span>,
    pub hint: Option<String>,
}

impl HqlError {
    fn new(message: impl Into<String>, span: Option<Span>, hint: Option<&str>) -> Self {
        Self {
            message: message.into(),
            span,
            hint: hint.map(str::to_owned),
        }
    }

    fn plain(message: impl Into<String>) -> Self {
        Self::new(message, None, None)
    }
}

#[derive(Clone, Debug)]
enum Numeric {
    Integer(i64),
    Float(f64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TokenKind {
    Word,
    String,
    Number,
    Operator,
    Punct,
    Eof,
}

#[derive(Clone, Debug)]
struct Token {
    kind: TokenKind,
    text: String,
    upper: String,
    numeric: Option<Numeric>,
    quoted: bool,
    span: Span,
}

fn tokenize(source: &str) -> Result<Vec<Token>, HqlError> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0;
    let mut line = 1;
    let mut line_start = 0;

    let make_span = |start: usize, end: usize, start_line: usize, start_line_start: usize| Span {
        start,
        end,
        line: start_line,
        column: start - start_line_start + 1,
    };

    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'\n' {
            index += 1;
            line += 1;
            line_start = index;
            continue;
        }
        if matches!(byte, b' ' | b'\t' | b'\r') {
            index += 1;
            continue;
        }

        if byte == b'-' && bytes.get(index + 1) == Some(&b'-') {
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
            continue;
        }
        if byte == b'/' && bytes.get(index + 1) == Some(&b'*') {
            let start = index;
            let start_line = line;
            let start_line_start = line_start;
            index += 2;
            while index + 1 < bytes.len() && !(bytes[index] == b'*' && bytes[index + 1] == b'/') {
                if bytes[index] == b'\n' {
                    line += 1;
                    line_start = index + 1;
                }
                index += 1;
            }
            if index + 1 >= bytes.len() {
                return Err(HqlError::new(
                    "unterminated block comment",
                    Some(make_span(start, index, start_line, start_line_start)),
                    None,
                ));
            }
            index += 2;
            continue;
        }

        let start = index;
        let start_line = line;
        let start_line_start = line_start;

        if byte == b'\'' {
            index += 1;
            let mut value = String::new();
            loop {
                if index >= bytes.len() {
                    return Err(HqlError::new(
                        "unterminated string literal",
                        Some(make_span(start, index, start_line, start_line_start)),
                        Some("add a closing '"),
                    ));
                }
                if bytes[index] == b'\'' {
                    if bytes.get(index + 1) == Some(&b'\'') {
                        value.push('\'');
                        index += 2;
                        continue;
                    }
                    index += 1;
                    break;
                }
                let ch = source[index..]
                    .chars()
                    .next()
                    .expect("valid UTF-8 boundary");
                if ch == '\n' {
                    line += 1;
                    line_start = index + 1;
                }
                value.push(ch);
                index += ch.len_utf8();
            }
            tokens.push(Token {
                kind: TokenKind::String,
                upper: value.to_uppercase(),
                text: value,
                numeric: None,
                quoted: false,
                span: make_span(start, index, start_line, start_line_start),
            });
            continue;
        }

        if byte == b'"' || byte == b'`' {
            let quote = byte;
            index += 1;
            let mut value = String::new();
            loop {
                if index >= bytes.len() || bytes[index] == b'\n' {
                    return Err(HqlError::new(
                        "unterminated quoted identifier",
                        Some(make_span(start, index, start_line, start_line_start)),
                        Some(if quote == b'"' {
                            "add a closing \""
                        } else {
                            "add a closing `"
                        }),
                    ));
                }
                if bytes[index] == quote {
                    // SQL-style doubled quote escapes the quote in an identifier.
                    if bytes.get(index + 1) == Some(&quote) {
                        value.push(quote as char);
                        index += 2;
                        continue;
                    }
                    index += 1;
                    break;
                }
                let ch = source[index..]
                    .chars()
                    .next()
                    .expect("valid UTF-8 boundary");
                value.push(ch);
                index += ch.len_utf8();
            }
            tokens.push(Token {
                kind: TokenKind::Word,
                text: value,
                upper: String::new(),
                numeric: None,
                quoted: true,
                span: make_span(start, index, start_line, start_line_start),
            });
            continue;
        }

        let next_is_digit = bytes.get(index + 1).is_some_and(u8::is_ascii_digit);
        if byte.is_ascii_digit() || (byte == b'-' && next_is_digit) {
            if byte == b'-' {
                index += 1;
            }
            while bytes.get(index).is_some_and(u8::is_ascii_digit) {
                index += 1;
            }
            let mut is_float = false;
            if bytes.get(index) == Some(&b'.')
                && bytes.get(index + 1).is_some_and(u8::is_ascii_digit)
            {
                is_float = true;
                index += 1;
                while bytes.get(index).is_some_and(u8::is_ascii_digit) {
                    index += 1;
                }
            }
            if matches!(bytes.get(index), Some(b'e' | b'E')) {
                let save = index;
                index += 1;
                if matches!(bytes.get(index), Some(b'+' | b'-')) {
                    index += 1;
                }
                if bytes.get(index).is_some_and(u8::is_ascii_digit) {
                    is_float = true;
                    while bytes.get(index).is_some_and(u8::is_ascii_digit) {
                        index += 1;
                    }
                } else {
                    index = save;
                }
            }
            let text = &source[start..index];
            let numeric = if is_float {
                let value = text.parse::<f64>().map_err(|_| {
                    HqlError::new(
                        "invalid numeric literal",
                        Some(make_span(start, index, start_line, start_line_start)),
                        None,
                    )
                })?;
                if !value.is_finite() {
                    return Err(HqlError::new(
                        "numeric literal is outside the supported range",
                        Some(make_span(start, index, start_line, start_line_start)),
                        None,
                    ));
                }
                Numeric::Float(value)
            } else {
                Numeric::Integer(text.parse::<i64>().map_err(|_| {
                    HqlError::new(
                        "integer literal is outside the signed 64-bit range",
                        Some(make_span(start, index, start_line, start_line_start)),
                        Some("HelixDB integer values must fit in i64"),
                    )
                })?)
            };
            tokens.push(Token {
                kind: TokenKind::Number,
                text: text.to_owned(),
                upper: text.to_owned(),
                numeric: Some(numeric),
                quoted: false,
                span: make_span(start, index, start_line, start_line_start),
            });
            continue;
        }

        if is_ident_start(byte) {
            index += 1;
            while bytes.get(index).is_some_and(|value| is_ident_part(*value)) {
                index += 1;
            }
            let text = &source[start..index];
            tokens.push(Token {
                kind: TokenKind::Word,
                text: text.to_owned(),
                upper: text.to_ascii_uppercase(),
                numeric: None,
                quoted: false,
                span: make_span(start, index, start_line, start_line_start),
            });
            continue;
        }

        let operator = [">=", "<=", "!=", "<>", "=", ">", "<"]
            .into_iter()
            .find(|operator| source[index..].starts_with(operator));
        if let Some(operator) = operator {
            index += operator.len();
            tokens.push(Token {
                kind: TokenKind::Operator,
                text: operator.to_owned(),
                upper: operator.to_owned(),
                numeric: None,
                quoted: false,
                span: make_span(start, index, start_line, start_line_start),
            });
            continue;
        }

        if matches!(byte, b'(' | b')' | b',' | b'*' | b'.' | b':' | b';') {
            index += 1;
            let text = (byte as char).to_string();
            tokens.push(Token {
                kind: TokenKind::Punct,
                upper: text.clone(),
                text,
                numeric: None,
                quoted: false,
                span: make_span(start, index, start_line, start_line_start),
            });
            continue;
        }

        let ch = source[index..]
            .chars()
            .next()
            .expect("valid UTF-8 boundary");
        index += ch.len_utf8();
        return Err(HqlError::new(
            format!("unexpected character {ch:?}"),
            Some(make_span(start, index, start_line, start_line_start)),
            None,
        ));
    }

    tokens.push(Token {
        kind: TokenKind::Eof,
        text: String::new(),
        upper: String::new(),
        numeric: None,
        quoted: false,
        span: Span {
            start: source.len(),
            end: source.len(),
            line,
            column: source.len() - line_start + 1,
        },
    });
    Ok(tokens)
}

fn is_ident_start(value: u8) -> bool {
    value.is_ascii_alphabetic() || matches!(value, b'_' | b'$')
}

fn is_ident_part(value: u8) -> bool {
    value.is_ascii_alphanumeric() || matches!(value, b'_' | b'$')
}

#[derive(Clone, Debug)]
enum Literal {
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Null,
}

#[derive(Clone, Debug)]
struct ColumnRef {
    name: String,
    source: String,
    virtual_column: bool,
    span: Span,
}

#[derive(Clone, Copy, Debug)]
enum ComparisonOperator {
    Eq,
    Neq,
    Gt,
    Gte,
    Lt,
    Lte,
}

#[derive(Clone, Debug)]
enum Condition {
    And(Vec<Condition>),
    Or(Vec<Condition>),
    Not(Box<Condition>),
    Compare {
        column: ColumnRef,
        operator: ComparisonOperator,
        value: Literal,
    },
    Between {
        column: ColumnRef,
        low: Literal,
        high: Literal,
    },
    In {
        column: ColumnRef,
        values: Vec<Literal>,
        negated: bool,
    },
    Like {
        column: ColumnRef,
        pattern: String,
        negated: bool,
    },
    IsNull {
        column: ColumnRef,
        negated: bool,
    },
    Has(ColumnRef),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EntityKind {
    Nodes,
    Edges,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HopDirection {
    Out,
    In,
    Both,
    OutE,
    InE,
    BothE,
    FromNode,
    ToNode,
    OtherNode,
}

#[derive(Clone, Debug)]
struct Hop {
    direction: HopDirection,
    label: Option<String>,
    condition: Option<Condition>,
    span: Span,
}

#[derive(Clone, Debug)]
struct Source {
    entity: EntityKind,
    label: Option<String>,
    span: Span,
}

#[derive(Clone, Debug)]
enum Projection {
    Star,
    Columns(Vec<ColumnRef>),
    Count,
}

#[derive(Clone, Debug)]
struct OrderTerm {
    column: ColumnRef,
    descending: bool,
}

#[derive(Clone, Debug)]
struct Selection {
    source: Source,
    condition: Option<Condition>,
    hops: Vec<Hop>,
    order_by: Vec<OrderTerm>,
    skip: Option<u64>,
    limit: Option<u64>,
    distinct: bool,
}

#[derive(Clone, Debug)]
struct SelectStatement {
    projection: Projection,
    group_by: Option<ColumnRef>,
    selection: Selection,
}

#[derive(Clone, Debug)]
struct GraphStatement {
    selection: Selection,
    with_properties: Vec<String>,
    edge_label: Option<String>,
    max_edges: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ShowTarget {
    Labels,
    NodeLabels,
    EdgeLabels,
    Stats,
}

#[derive(Clone, Debug)]
struct ShowStatement {
    target: ShowTarget,
    sample: u64,
}

#[derive(Clone, Debug)]
struct DescribeStatement {
    entity: EntityKind,
    id: i64,
    limit: u64,
}

#[derive(Clone, Debug)]
enum Statement {
    Select(SelectStatement),
    Graph(GraphStatement),
    Show(ShowStatement),
    Describe(DescribeStatement),
}

struct Parser {
    tokens: Vec<Token>,
    position: usize,
}

impl Parser {
    fn new(source: &str) -> Result<Self, HqlError> {
        Ok(Self {
            tokens: tokenize(source)?,
            position: 0,
        })
    }

    fn peek(&self, offset: usize) -> &Token {
        &self.tokens[(self.position + offset).min(self.tokens.len() - 1)]
    }

    fn next(&mut self) -> Token {
        let token = self.peek(0).clone();
        if token.kind != TokenKind::Eof {
            self.position += 1;
        }
        token
    }

    fn at_end(&self) -> bool {
        self.peek(0).kind == TokenKind::Eof
    }

    fn is_keyword(&self, word: &str) -> bool {
        let token = self.peek(0);
        token.kind == TokenKind::Word && !token.quoted && token.upper == word
    }

    fn take_keyword(&mut self, word: &str) -> bool {
        if !self.is_keyword(word) {
            return false;
        }
        self.position += 1;
        true
    }

    fn expect_keyword(&mut self, word: &str, context: &str) -> Result<Token, HqlError> {
        if !self.is_keyword(word) {
            return Err(self.error_at(self.peek(0), format!("expected {word} {context}"), None));
        }
        Ok(self.next())
    }

    fn take_punct(&mut self, value: char) -> bool {
        let token = self.peek(0);
        if token.kind == TokenKind::Punct && token.text == value.to_string() {
            self.position += 1;
            return true;
        }
        false
    }

    fn expect_punct(&mut self, value: char, context: &str) -> Result<Token, HqlError> {
        if !self.take_punct(value) {
            return Err(self.error_at(self.peek(0), format!("expected {value:?} {context}"), None));
        }
        Ok(self.tokens[self.position - 1].clone())
    }

    fn error_at(&self, token: &Token, message: impl Into<String>, hint: Option<&str>) -> HqlError {
        let found = if token.kind == TokenKind::Eof {
            "end of query".to_owned()
        } else {
            format!("{:?}", token.text)
        };
        HqlError::new(
            format!("{}, found {found}", message.into()),
            Some(token.span.clone()),
            hint,
        )
    }

    fn parse(mut self) -> Result<Statement, HqlError> {
        if self.at_end() {
            return Err(HqlError::new(
                "empty query",
                Some(self.peek(0).span.clone()),
                Some("try: SELECT * FROM NODES LIMIT 25"),
            ));
        }

        let statement = if self.is_keyword("SELECT") {
            Statement::Select(self.parse_select()?)
        } else if self.is_keyword("QUERY") || self.is_keyword("GRAPH") {
            Statement::Graph(self.parse_graph()?)
        } else if self.is_keyword("SHOW") {
            Statement::Show(self.parse_show()?)
        } else if self.is_keyword("DESCRIBE") {
            Statement::Describe(self.parse_describe()?)
        } else {
            return Err(self.error_at(
                self.peek(0),
                "expected a statement (SELECT, QUERY, SHOW or DESCRIBE)",
                Some("HelixSQL is read-only; writes must go through a HelixDB SDK"),
            ));
        };

        self.take_punct(';');
        if !self.at_end() {
            return Err(self.error_at(
                self.peek(0),
                "unexpected trailing input",
                Some("only one statement per query"),
            ));
        }
        Ok(statement)
    }

    fn parse_select(&mut self) -> Result<SelectStatement, HqlError> {
        self.expect_keyword("SELECT", "at the start of the statement")?;
        let distinct = self.take_keyword("DISTINCT");
        let projection = self.parse_projection()?;
        self.expect_keyword("FROM", "after the select list")?;
        let source = self.parse_source()?;
        let condition = if self.take_keyword("WHERE") {
            Some(self.parse_condition()?)
        } else {
            None
        };
        let hops = self.parse_hops()?;
        let group_by = if self.take_keyword("GROUP") {
            self.expect_keyword("BY", "after GROUP")?;
            Some(self.parse_column()?)
        } else {
            None
        };
        let order_by = self.parse_order_by()?;
        let (skip, limit) = self.parse_paging()?;

        if let Some(column) = &group_by {
            if !matches!(projection, Projection::Count) {
                return Err(HqlError::new(
                    "GROUP BY is only supported with COUNT(*)",
                    Some(column.span.clone()),
                    Some("try: SELECT COUNT(*) FROM NODES GROUP BY label"),
                ));
            }
        }
        if matches!(projection, Projection::Count) && !order_by.is_empty() {
            return Err(HqlError::new(
                "ORDER BY cannot be combined with COUNT(*)",
                Some(order_by[0].column.span.clone()),
                Some("a count returns a single row"),
            ));
        }

        Ok(SelectStatement {
            projection,
            group_by,
            selection: Selection {
                source,
                condition,
                hops,
                order_by,
                skip,
                limit,
                distinct,
            },
        })
    }

    fn parse_projection(&mut self) -> Result<Projection, HqlError> {
        if self.take_punct('*') {
            return Ok(Projection::Star);
        }
        if self.is_keyword("COUNT")
            && self.peek(1).kind == TokenKind::Punct
            && self.peek(1).text == "("
        {
            self.next();
            self.expect_punct('(', "after COUNT")?;
            if !self.take_punct('*') {
                return Err(self.error_at(
                    self.peek(0),
                    "expected * inside COUNT",
                    Some("only COUNT(*) is supported; use GROUP BY to break the count down"),
                ));
            }
            self.expect_punct(')', "to close COUNT(*)")?;
            return Ok(Projection::Count);
        }

        let mut columns = vec![self.parse_column()?];
        while self.take_punct(',') {
            columns.push(self.parse_column()?);
        }
        Ok(Projection::Columns(columns))
    }

    fn parse_column(&mut self) -> Result<ColumnRef, HqlError> {
        let token = self.peek(0).clone();
        if token.kind != TokenKind::Word {
            return Err(self.error_at(&token, "expected a column name", None));
        }
        if !token.quoted && is_keyword_word(&token.upper) && !is_column_safe_keyword(&token.upper) {
            let hint = if matches!(token.upper.as_str(), "FROM" | "TO") {
                "use source / target for an edge's endpoints".to_owned()
            } else {
                format!(
                    "quote it as \"{}\" to use it as a property name",
                    token.text
                )
            };
            return Err(self.error_at(
                &token,
                format!(
                    "expected a column name but {} is a keyword",
                    token.text.to_ascii_uppercase()
                ),
                Some(&hint),
            ));
        }
        self.next();

        if !token.quoted && token.text.starts_with('$') {
            let mut name = token.text.clone();
            let mut span = token.span.clone();
            while self.peek(0).kind == TokenKind::Punct
                && self.peek(0).text == "."
                && self.peek(1).kind == TokenKind::Word
            {
                self.next();
                let part = self.next();
                name.push('.');
                name.push_str(&part.text);
                span.end = part.span.end;
            }
            return Ok(ColumnRef {
                name: name.clone(),
                source: name,
                virtual_column: true,
                span,
            });
        }
        Ok(make_column(token.text, token.span, token.quoted))
    }

    fn parse_source(&mut self) -> Result<Source, HqlError> {
        let token = self.peek(0).clone();
        let entity = if self.is_keyword("NODES") || self.is_keyword("NODE") {
            EntityKind::Nodes
        } else if self.is_keyword("EDGES") || self.is_keyword("EDGE") {
            EntityKind::Edges
        } else {
            return Err(self.error_at(
                &token,
                "expected NODES or EDGES",
                Some("try: SELECT * FROM NODES:User LIMIT 25"),
            ));
        };
        self.next();
        let label = if self.take_punct(':') {
            Some(self.parse_label("after ':'")?)
        } else if self.is_label_token() {
            Some(self.parse_label("after the source")?)
        } else {
            None
        };
        let end = self.tokens[self.position - 1].span.end;
        Ok(Source {
            entity,
            label,
            span: Span { end, ..token.span },
        })
    }

    fn is_label_token(&self) -> bool {
        let token = self.peek(0);
        token.kind == TokenKind::String
            || (token.kind == TokenKind::Word && (token.quoted || !is_keyword_word(&token.upper)))
    }

    fn parse_label(&mut self, context: &str) -> Result<String, HqlError> {
        let token = self.peek(0).clone();
        if matches!(token.kind, TokenKind::String | TokenKind::Word) {
            if token.kind == TokenKind::Word && !token.quoted && is_keyword_word(&token.upper) {
                let hint = format!("quote it as \"{}\"", token.text);
                return Err(self.error_at(
                    &token,
                    format!(
                        "expected a label {context} but {} is a keyword",
                        token.text.to_ascii_uppercase()
                    ),
                    Some(&hint),
                ));
            }
            self.next();
            return Ok(token.text);
        }
        Err(self.error_at(&token, format!("expected a label {context}"), None))
    }

    fn parse_hops(&mut self) -> Result<Vec<Hop>, HqlError> {
        let mut hops = Vec::new();
        while self.is_keyword("TRAVERSE") {
            let start = self.next().span;
            let direction = self.parse_hop_direction()?;
            let endpoint_hop = matches!(
                direction,
                HopDirection::FromNode | HopDirection::ToNode | HopDirection::OtherNode
            );
            let label = if !endpoint_hop && self.is_label_token() {
                Some(self.parse_label("after the direction")?)
            } else {
                None
            };
            let condition = if self.take_keyword("WHERE") {
                Some(self.parse_condition()?)
            } else {
                None
            };
            let end = self.tokens[self.position - 1].span.end;
            hops.push(Hop {
                direction,
                label,
                condition,
                span: Span { end, ..start },
            });
        }
        Ok(hops)
    }

    fn parse_hop_direction(&mut self) -> Result<HopDirection, HqlError> {
        let token = self.peek(0).clone();
        if token.kind != TokenKind::Word || token.quoted {
            return Err(self.error_at(
                &token,
                "expected a traversal direction",
                Some(DIRECTION_HINT),
            ));
        }
        if matches!(token.upper.as_str(), "OUT" | "IN" | "BOTH") {
            self.next();
            let to_edges = self.is_keyword("EDGES") || self.is_keyword("EDGE");
            if to_edges {
                self.next();
            }
            return Ok(match (token.upper.as_str(), to_edges) {
                ("OUT", false) => HopDirection::Out,
                ("OUT", true) => HopDirection::OutE,
                ("IN", false) => HopDirection::In,
                ("IN", true) => HopDirection::InE,
                ("BOTH", false) => HopDirection::Both,
                ("BOTH", true) => HopDirection::BothE,
                _ => unreachable!(),
            });
        }
        let direction = match token.upper.as_str() {
            "SOURCE" => HopDirection::FromNode,
            "TARGET" => HopDirection::ToNode,
            "OTHER" => HopDirection::OtherNode,
            _ => {
                return Err(self.error_at(
                    &token,
                    "expected a traversal direction",
                    Some(DIRECTION_HINT),
                ))
            }
        };
        self.next();
        Ok(direction)
    }

    fn parse_order_by(&mut self) -> Result<Vec<OrderTerm>, HqlError> {
        if !self.take_keyword("ORDER") {
            return Ok(Vec::new());
        }
        self.expect_keyword("BY", "after ORDER")?;
        let mut terms = Vec::new();
        loop {
            let column = self.parse_column()?;
            let descending = if self.take_keyword("DESC") {
                true
            } else {
                self.take_keyword("ASC");
                false
            };
            terms.push(OrderTerm { column, descending });
            if !self.take_punct(',') {
                break;
            }
        }
        Ok(terms)
    }

    fn parse_paging(&mut self) -> Result<(Option<u64>, Option<u64>), HqlError> {
        let mut skip = None;
        let mut limit = None;
        for _ in 0..2 {
            if (self.is_keyword("SKIP") || self.is_keyword("OFFSET")) && skip.is_none() {
                let keyword = self.next().upper;
                skip = Some(self.parse_count(&keyword)?);
                continue;
            }
            if self.is_keyword("LIMIT") && limit.is_none() {
                self.next();
                limit = Some(self.parse_count("LIMIT")?);
                continue;
            }
            break;
        }
        Ok((skip, limit))
    }

    fn parse_count(&mut self, keyword: &str) -> Result<u64, HqlError> {
        let token = self.peek(0).clone();
        let Some(Numeric::Integer(value)) = token.numeric else {
            return Err(self.error_at(
                &token,
                format!("expected a whole number after {keyword}"),
                None,
            ));
        };
        if value < 0 {
            return Err(self.error_at(&token, format!("{keyword} cannot be negative"), None));
        }
        self.next();
        Ok(value as u64)
    }

    fn parse_condition(&mut self) -> Result<Condition, HqlError> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Result<Condition, HqlError> {
        let mut parts = vec![self.parse_and()?];
        while self.take_keyword("OR") {
            parts.push(self.parse_and()?);
        }
        Ok(if parts.len() == 1 {
            parts.remove(0)
        } else {
            Condition::Or(parts)
        })
    }

    fn parse_and(&mut self) -> Result<Condition, HqlError> {
        let mut parts = vec![self.parse_not()?];
        while self.take_keyword("AND") {
            parts.push(self.parse_not()?);
        }
        Ok(if parts.len() == 1 {
            parts.remove(0)
        } else {
            Condition::And(parts)
        })
    }

    fn parse_not(&mut self) -> Result<Condition, HqlError> {
        if self.take_keyword("NOT") {
            return Ok(Condition::Not(Box::new(self.parse_not()?)));
        }
        self.parse_primary_condition()
    }

    fn parse_primary_condition(&mut self) -> Result<Condition, HqlError> {
        if self.take_punct('(') {
            let inner = self.parse_condition()?;
            self.expect_punct(')', "to close the group")?;
            return Ok(inner);
        }
        if self.is_keyword("HAS")
            && self.peek(1).kind == TokenKind::Punct
            && self.peek(1).text == "("
        {
            self.next();
            self.expect_punct('(', "after HAS")?;
            let column = self.parse_column()?;
            self.expect_punct(')', "to close HAS(...)")?;
            return Ok(Condition::Has(column));
        }

        let column = self.parse_column()?;
        if self.take_keyword("IS") {
            let negated = self.take_keyword("NOT");
            self.expect_keyword("NULL", "after IS")?;
            return Ok(Condition::IsNull { column, negated });
        }
        let negated = self.take_keyword("NOT");
        if self.take_keyword("IN") {
            self.expect_punct('(', "after IN")?;
            let mut values = Vec::new();
            if !self.take_punct(')') {
                loop {
                    values.push(self.parse_literal()?);
                    if !self.take_punct(',') {
                        break;
                    }
                }
                self.expect_punct(')', "to close the IN list")?;
            }
            if values.is_empty() {
                return Err(HqlError::new(
                    "IN needs at least one value",
                    Some(column.span.clone()),
                    None,
                ));
            }
            return Ok(Condition::In {
                column,
                values,
                negated,
            });
        }
        if self.take_keyword("LIKE") {
            let token = self.peek(0).clone();
            if token.kind != TokenKind::String {
                return Err(self.error_at(
                    &token,
                    "expected a quoted pattern after LIKE",
                    Some("for example: LIKE 'ali%'"),
                ));
            }
            self.next();
            return Ok(Condition::Like {
                column,
                pattern: token.text,
                negated,
            });
        }
        if self.take_keyword("BETWEEN") {
            let low = self.parse_literal()?;
            self.expect_keyword("AND", "between the BETWEEN bounds")?;
            let high = self.parse_literal()?;
            let condition = Condition::Between { column, low, high };
            return Ok(if negated {
                Condition::Not(Box::new(condition))
            } else {
                condition
            });
        }
        if negated {
            return Err(self.error_at(
                self.peek(0),
                "expected IN, LIKE or BETWEEN after NOT",
                None,
            ));
        }

        let operator_token = self.peek(0).clone();
        if operator_token.kind != TokenKind::Operator {
            return Err(self.error_at(
                &operator_token,
                "expected a comparison operator",
                Some("one of =, !=, >, >=, <, <=, IN, LIKE, BETWEEN, IS NULL"),
            ));
        }
        self.next();
        let operator = match operator_token.text.as_str() {
            "=" => ComparisonOperator::Eq,
            "!=" | "<>" => ComparisonOperator::Neq,
            ">" => ComparisonOperator::Gt,
            ">=" => ComparisonOperator::Gte,
            "<" => ComparisonOperator::Lt,
            "<=" => ComparisonOperator::Lte,
            _ => unreachable!(),
        };
        Ok(Condition::Compare {
            column,
            operator,
            value: self.parse_literal()?,
        })
    }

    fn parse_literal(&mut self) -> Result<Literal, HqlError> {
        let token = self.peek(0).clone();
        if token.kind == TokenKind::String {
            self.next();
            return Ok(Literal::String(token.text));
        }
        if token.kind == TokenKind::Number {
            self.next();
            return Ok(match token.numeric.expect("number token") {
                Numeric::Integer(value) => Literal::Integer(value),
                Numeric::Float(value) => Literal::Float(value),
            });
        }
        if token.kind == TokenKind::Word && !token.quoted {
            match token.upper.as_str() {
                "TRUE" => {
                    self.next();
                    return Ok(Literal::Boolean(true));
                }
                "FALSE" => {
                    self.next();
                    return Ok(Literal::Boolean(false));
                }
                "NULL" => {
                    self.next();
                    return Ok(Literal::Null);
                }
                _ => {}
            }
        }
        Err(self.error_at(
            &token,
            "expected a literal value",
            Some("strings use single quotes: WHERE name = 'Alice'"),
        ))
    }

    fn parse_graph(&mut self) -> Result<GraphStatement, HqlError> {
        if !self.take_keyword("QUERY") {
            self.expect_keyword("GRAPH", "at the start of the statement")?;
        }
        let keyword_span = self.tokens[self.position - 1].span.clone();
        let source =
            if self.take_keyword("FROM") || self.is_keyword("NODES") || self.is_keyword("NODE") {
                self.parse_source()?
            } else {
                Source {
                    entity: EntityKind::Nodes,
                    label: None,
                    span: keyword_span,
                }
            };
        if source.entity != EntityKind::Nodes {
            return Err(HqlError::new(
                "QUERY selects nodes; edges are pulled in around them",
                Some(source.span.clone()),
                Some("use VIA <Label> to restrict which edges are drawn"),
            ));
        }
        let condition = if self.take_keyword("WHERE") {
            Some(self.parse_condition()?)
        } else {
            None
        };
        let hops = self.parse_hops()?;
        let edge_label = if self.take_keyword("VIA") {
            Some(self.parse_label("after VIA")?)
        } else {
            None
        };
        let mut with_properties = Vec::new();
        if self.take_keyword("WITH") {
            loop {
                with_properties.push(self.parse_column()?.name);
                if !self.take_punct(',') {
                    break;
                }
            }
        }
        let order_by = self.parse_order_by()?;
        let (skip, limit) = self.parse_paging()?;
        let max_edges = if self.is_keyword("EDGE") || self.is_keyword("EDGES") {
            self.next();
            self.expect_keyword("LIMIT", "after EDGE")?;
            Some(self.parse_count("EDGE LIMIT")?)
        } else {
            None
        };
        if let Some(last) = hops.last() {
            if matches!(
                last.direction,
                HopDirection::OutE | HopDirection::InE | HopDirection::BothE
            ) {
                return Err(HqlError::new(
                    "the last TRAVERSE of a QUERY must land on nodes",
                    Some(last.span.clone()),
                    Some("drop the EDGES keyword, or add TRAVERSE TARGET to step back onto nodes"),
                ));
            }
        }
        Ok(GraphStatement {
            selection: Selection {
                source,
                condition,
                hops,
                order_by,
                skip,
                limit,
                distinct: true,
            },
            with_properties,
            edge_label,
            max_edges,
        })
    }

    fn parse_show(&mut self) -> Result<ShowStatement, HqlError> {
        self.expect_keyword("SHOW", "at the start of the statement")?;
        let target = if self.take_keyword("STATS") {
            ShowTarget::Stats
        } else if self.is_keyword("NODE") || self.is_keyword("NODES") {
            self.next();
            self.expect_keyword("LABELS", "after NODE")?;
            ShowTarget::NodeLabels
        } else if self.is_keyword("EDGE") || self.is_keyword("EDGES") {
            self.next();
            self.expect_keyword("LABELS", "after EDGE")?;
            ShowTarget::EdgeLabels
        } else if self.take_keyword("LABELS") {
            ShowTarget::Labels
        } else {
            return Err(self.error_at(
                self.peek(0),
                "expected LABELS, NODE LABELS, EDGE LABELS or STATS",
                Some("try: SHOW LABELS"),
            ));
        };
        let sample = if self.take_keyword("SAMPLE") {
            self.parse_count("SAMPLE")?
        } else {
            5_000
        };
        Ok(ShowStatement { target, sample })
    }

    fn parse_describe(&mut self) -> Result<DescribeStatement, HqlError> {
        self.expect_keyword("DESCRIBE", "at the start of the statement")?;
        let entity = if self.is_keyword("NODE") || self.is_keyword("NODES") {
            EntityKind::Nodes
        } else if self.is_keyword("EDGE") || self.is_keyword("EDGES") {
            EntityKind::Edges
        } else {
            return Err(self.error_at(
                self.peek(0),
                "expected NODE or EDGE",
                Some("try: DESCRIBE NODE 42"),
            ));
        };
        self.next();
        let token = self.peek(0).clone();
        let id = if let Some(Numeric::Integer(value)) = token.numeric {
            value
        } else if token.kind == TokenKind::String {
            token.text.parse::<i64>().map_err(|_| {
                self.error_at(
                    &token,
                    "expected a numeric entity id",
                    Some("HelixDB ids are 64-bit integers"),
                )
            })?
        } else {
            return Err(self.error_at(
                &token,
                "expected a numeric entity id",
                Some("HelixDB ids are 64-bit integers"),
            ));
        };
        self.next();
        let limit = if self.take_keyword("LIMIT") {
            self.parse_count("LIMIT")?
        } else {
            200
        };
        Ok(DescribeStatement { entity, id, limit })
    }
}

const DIRECTION_HINT: &str = "one of OUT, IN, BOTH (to nodes), OUT EDGES, IN EDGES, BOTH EDGES (to edges), SOURCE, TARGET, OTHER (edge to node)";

pub(crate) fn is_keyword_word(word: &str) -> bool {
    matches!(
        word,
        "SELECT"
            | "DISTINCT"
            | "FROM"
            | "WHERE"
            | "TRAVERSE"
            | "ORDER"
            | "BY"
            | "ASC"
            | "DESC"
            | "LIMIT"
            | "SKIP"
            | "OFFSET"
            | "GROUP"
            | "AND"
            | "OR"
            | "NOT"
            | "IN"
            | "IS"
            | "NULL"
            | "LIKE"
            | "BETWEEN"
            | "HAS"
            | "COUNT"
            | "TRUE"
            | "FALSE"
            | "NODES"
            | "EDGES"
            | "NODE"
            | "EDGE"
            | "GRAPH"
            | "SHOW"
            | "DESCRIBE"
            | "LABELS"
            | "STATS"
            | "SAMPLE"
            | "WITH"
            | "VIA"
            | "OUT"
            | "BOTH"
            | "SOURCE"
            | "TARGET"
            | "OTHER"
    )
}

fn is_column_safe_keyword(word: &str) -> bool {
    matches!(word, "SOURCE" | "TARGET" | "LABELS" | "STATS")
}

fn make_column(name: String, span: Span, quoted: bool) -> ColumnRef {
    let virtual_source = if !quoted {
        match name.to_ascii_lowercase().as_str() {
            "id" => Some("$id"),
            "label" => Some("$label"),
            "source" => Some("$from.$id"),
            "target" => Some("$to.$id"),
            "score" => Some("$score"),
            "distance" => Some("$distance"),
            _ => None,
        }
    } else {
        None
    };
    ColumnRef {
        source: virtual_source.unwrap_or(&name).to_owned(),
        virtual_column: virtual_source.is_some(),
        name,
        span,
    }
}

#[derive(Clone, Debug)]
pub enum ResultShape {
    Rows {
        variable: String,
        columns: Option<Vec<String>>,
        identity_variable: Option<String>,
    },
    Count {
        variable: String,
    },
    GroupCount {
        variable: String,
        by: String,
    },
    Graph {
        node_variable: String,
        edge_variable: String,
        node_limit: u64,
        edge_limit: u64,
    },
    Labels {
        nodes: Option<String>,
        edges: Option<String>,
    },
    Stats {
        node_count: String,
        edge_count: String,
        node_labels: String,
        edge_labels: String,
    },
    DescribeNode,
    DescribeEdge,
}

#[derive(Clone, Debug)]
pub struct CompiledQuery {
    pub transport_json: String,
    pub summary: String,
    pub shape: ResultShape,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompiledQueryView {
    pub transport_json: String,
    pub summary: String,
}

impl From<&CompiledQuery> for CompiledQueryView {
    fn from(value: &CompiledQuery) -> Self {
        Self {
            transport_json: value.transport_json.clone(),
            summary: value.summary.clone(),
        }
    }
}

pub fn compile_source(source: &str) -> Result<CompiledQuery, HqlError> {
    let statement = Parser::new(source)?.parse()?;
    compile_statement(&statement)
}

fn compile_statement(statement: &Statement) -> Result<CompiledQuery, HqlError> {
    let (queries, shape, summary, kind) = match statement {
        Statement::Select(statement) => compile_select(statement)?,
        Statement::Graph(statement) => compile_graph(statement)?,
        Statement::Show(statement) => compile_show(statement),
        Statement::Describe(statement) => compile_describe(statement),
    };
    let returns = queries
        .iter()
        .filter_map(|query| query.pointer("/Query/name").and_then(Value::as_str))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let request = json!({
        "request_type": "read",
        "query_name": format!("helix_visualizer_{kind}"),
        "query": { "queries": queries, "returns": returns },
        "parameters": {},
    });
    Ok(CompiledQuery {
        transport_json: serde_json::to_string_pretty(&request)
            .map_err(|error| HqlError::plain(error.to_string()))?,
        summary,
        shape,
    })
}

type CompileParts = (Vec<Value>, ResultShape, String, &'static str);

fn compile_select(statement: &SelectStatement) -> Result<CompileParts, HqlError> {
    let landing = validate_hops(&statement.selection)?;
    validate_condition(statement.selection.condition.as_ref())?;
    for hop in &statement.selection.hops {
        validate_condition(hop.condition.as_ref())?;
    }

    let description = describe_selection(&statement.selection);
    if matches!(statement.projection, Projection::Count) {
        let terminal = if let Some(group_by) = &statement.group_by {
            json!({ "GroupCount": group_by.source })
        } else {
            json!("Count")
        };
        let mut steps = selection_steps(&statement.selection, None)?;
        steps.push(terminal);
        if let Some(group_by) = &statement.group_by {
            return Ok((
                vec![named("rows", steps)],
                ResultShape::GroupCount {
                    variable: "rows".into(),
                    by: group_by.name.clone(),
                },
                format!("count of {description} grouped by {}", group_by.name),
                "group_count",
            ));
        }
        return Ok((
            vec![named("rows", steps)],
            ResultShape::Count {
                variable: "rows".into(),
            },
            format!("count of {description}"),
            "count",
        ));
    }

    let terminal = terminal_for(landing);
    let mut steps = selection_steps(&statement.selection, Some(DEFAULT_ROW_LIMIT))?;
    steps.push(terminal.clone());
    match &statement.projection {
        Projection::Star => {
            let mut identity = selection_steps(&statement.selection, Some(DEFAULT_ROW_LIMIT))?;
            identity.push(terminal);
            Ok((
                vec![named("rows", steps), named("identity", identity)],
                ResultShape::Rows {
                    variable: "rows".into(),
                    columns: None,
                    identity_variable: Some("identity".into()),
                },
                format!("all properties of {description}"),
                "select_star",
            ))
        }
        Projection::Columns(columns) => Ok((
            vec![named("rows", steps)],
            ResultShape::Rows {
                variable: "rows".into(),
                columns: Some(columns.iter().map(|column| column.name.clone()).collect()),
                identity_variable: None,
            },
            format!(
                "{} column{} of {description}",
                columns.len(),
                if columns.len() == 1 { "" } else { "s" }
            ),
            "select",
        )),
        Projection::Count => unreachable!(),
    }
}

fn compile_graph(statement: &GraphStatement) -> Result<CompileParts, HqlError> {
    if validate_hops(&statement.selection)? != EntityKind::Nodes {
        return Err(HqlError::plain("a QUERY must finish on nodes"));
    }
    validate_condition(statement.selection.condition.as_ref())?;
    for hop in &statement.selection.hops {
        validate_condition(hop.condition.as_ref())?;
    }
    let node_limit = statement
        .selection
        .limit
        .unwrap_or(DEFAULT_GRAPH_NODE_LIMIT);
    let edge_limit = statement.max_edges.unwrap_or(DEFAULT_GRAPH_EDGE_LIMIT);
    let mut selection = statement.selection.clone();
    selection.limit = Some(node_limit);
    let mut nodes = selection_steps(&selection, Some(node_limit))?;
    nodes.push(json!({ "ValueMap": null }));
    let mut edges = selection_steps(&selection, Some(node_limit))?;
    edges.push(json!({ "BothE": statement.edge_label }));
    edges.push(json!("Dedup"));
    edges.push(json!({ "Limit": edge_limit }));
    edges.push(json!("EdgeProperties"));
    let property_note = if statement.with_properties.is_empty() {
        String::new()
    } else {
        format!(
            " with {} requested properties",
            statement.with_properties.len()
        )
    };
    Ok((
        vec![named("nodes", nodes), named("edges", edges)],
        ResultShape::Graph {
            node_variable: "nodes".into(),
            edge_variable: "edges".into(),
            node_limit,
            edge_limit,
        },
        format!(
            "graph of {} (≤{node_limit} nodes, ≤{edge_limit} edges){property_note}",
            describe_selection(&statement.selection)
        ),
        "graph",
    ))
}

fn compile_show(statement: &ShowStatement) -> CompileParts {
    let sample = statement.sample.max(1);
    let node_labels = || {
        named(
            "nodeLabels",
            vec![
                json!({ "N": "All" }),
                json!({ "Limit": sample }),
                json!({ "GroupCount": "$label" }),
            ],
        )
    };
    let edge_source = || {
        vec![
            json!({ "N": "All" }),
            json!({ "OutE": null }),
            json!("Dedup"),
        ]
    };
    let edge_labels = || {
        let mut steps = edge_source();
        steps.push(json!({ "Limit": sample }));
        steps.push(json!("EdgeProperties"));
        named("edgeLabels", steps)
    };
    if statement.target == ShowTarget::Stats {
        let mut edge_count = edge_source();
        edge_count.push(json!("Count"));
        return (
            vec![
                named("nodeCount", vec![json!({ "N": "All" }), json!("Count")]),
                named("edgeCount", edge_count),
                node_labels(),
                edge_labels(),
            ],
            ResultShape::Stats {
                node_count: "nodeCount".into(),
                edge_count: "edgeCount".into(),
                node_labels: "nodeLabels".into(),
                edge_labels: "edgeLabels".into(),
            },
            format!("instance totals, with labels sampled over {sample} entities"),
            "stats",
        );
    }
    let want_nodes = statement.target != ShowTarget::EdgeLabels;
    let want_edges = statement.target != ShowTarget::NodeLabels;
    let mut queries = Vec::new();
    if want_nodes {
        queries.push(node_labels());
    }
    if want_edges {
        queries.push(edge_labels());
    }
    (
        queries,
        ResultShape::Labels {
            nodes: want_nodes.then(|| "nodeLabels".into()),
            edges: want_edges.then(|| "edgeLabels".into()),
        },
        format!("labels seen across a sample of {sample} entities"),
        "labels",
    )
}

fn compile_describe(statement: &DescribeStatement) -> CompileParts {
    let id = statement.id;
    if statement.entity == EntityKind::Edges {
        let edge = || vec![json!({ "E": { "Ids": [id] } })];
        let mut entity = edge();
        entity.push(json!("EdgeProperties"));
        let mut identity = edge();
        identity.push(json!("EdgeProperties"));
        let mut source = edge();
        source.push(json!({ "OutN": null }));
        source.push(json!({ "ValueMap": null }));
        let mut target = edge();
        target.push(json!({ "InN": null }));
        target.push(json!({ "ValueMap": null }));
        return (
            vec![
                named("entity", entity),
                named("identity", identity),
                named("sourceNode", source),
                named("targetNode", target),
            ],
            ResultShape::DescribeEdge,
            format!("edge {id} with its endpoints"),
            "describe_edge",
        );
    }
    let node = || vec![json!({ "N": { "Ids": [id] } })];
    let mut entity = node();
    entity.push(json!({ "ValueMap": null }));
    let mut identity = node();
    identity.push(json!({ "ValueMap": null }));
    let mut edges = node();
    edges.push(json!({ "BothE": null }));
    let mut degree = edges.clone();
    degree.push(json!("Count"));
    edges.push(json!({ "Limit": statement.limit }));
    edges.push(json!("EdgeProperties"));
    let mut neighbours = node();
    neighbours.push(json!({ "Both": null }));
    neighbours.push(json!("Dedup"));
    neighbours.push(json!({ "Limit": statement.limit }));
    neighbours.push(json!({ "ValueMap": null }));
    (
        vec![
            named("entity", entity),
            named("identity", identity),
            named("edges", edges),
            named("neighbours", neighbours),
            named("degree", degree),
        ],
        ResultShape::DescribeNode,
        format!("node {id} with up to {} incident edges", statement.limit),
        "describe_node",
    )
}

fn named(name: &str, steps: Vec<Value>) -> Value {
    json!({ "Query": { "name": name, "steps": steps, "condition": null } })
}

fn selection_steps(
    selection: &Selection,
    default_limit: Option<u64>,
) -> Result<Vec<Value>, HqlError> {
    let mut steps = source_steps(
        selection.source.entity,
        selection.source.label.as_deref(),
        selection.condition.as_ref(),
    )?;
    for hop in &selection.hops {
        steps.push(hop_step(hop.direction, hop.label.as_deref()));
        if let Some(condition) = &hop.condition {
            steps.push(json!({ "Where": predicate(condition)? }));
        }
    }
    if selection.distinct {
        steps.push(json!("Dedup"));
    }
    for term in &selection.order_by {
        steps.push(json!({
            "OrderBy": [term.column.source, if term.descending { "Desc" } else { "Asc" }]
        }));
    }
    if let Some(skip) = selection.skip {
        steps.push(json!({ "Skip": skip }));
    }
    if let Some(limit) = selection.limit.or(default_limit) {
        steps.push(json!({ "Limit": limit }));
    }
    Ok(steps)
}

fn source_steps(
    entity: EntityKind,
    label: Option<&str>,
    condition: Option<&Condition>,
) -> Result<Vec<Value>, HqlError> {
    let mut filters = Vec::new();
    if let Some(label) = label {
        filters.push(json!({ "Eq": ["$label", { "String": label }] }));
    }
    if let Some(condition) = condition {
        filters.push(predicate(condition)?);
    }
    let combined = match filters.len() {
        0 => None,
        1 => filters.pop(),
        _ => Some(json!({ "And": filters })),
    };
    if let Some(combined) = combined {
        return Ok(vec![if entity == EntityKind::Nodes {
            json!({ "NWhere": combined })
        } else {
            json!({ "EWhere": combined })
        }]);
    }
    Ok(if entity == EntityKind::Nodes {
        vec![json!({ "N": "All" })]
    } else {
        vec![
            json!({ "N": "All" }),
            json!({ "OutE": null }),
            json!("Dedup"),
        ]
    })
}

fn hop_step(direction: HopDirection, label: Option<&str>) -> Value {
    let name = match direction {
        HopDirection::Out => "Out",
        HopDirection::In => "In",
        HopDirection::Both => "Both",
        HopDirection::OutE => "OutE",
        HopDirection::InE => "InE",
        HopDirection::BothE => "BothE",
        HopDirection::FromNode => "OutN",
        HopDirection::ToNode => "InN",
        HopDirection::OtherNode => "OtherN",
    };
    let mut object = Map::new();
    object.insert(
        name.to_owned(),
        label.map_or(Value::Null, |value| json!(value)),
    );
    Value::Object(object)
}

fn predicate(condition: &Condition) -> Result<Value, HqlError> {
    Ok(match condition {
        Condition::And(parts) => json!({
            "And": parts.iter().map(predicate).collect::<Result<Vec<_>, _>>()?
        }),
        Condition::Or(parts) => json!({
            "Or": parts.iter().map(predicate).collect::<Result<Vec<_>, _>>()?
        }),
        Condition::Not(part) => json!({ "Not": predicate(part)? }),
        Condition::Compare {
            column,
            operator,
            value,
        } => {
            if matches!(value, Literal::Null) {
                let name = match operator {
                    ComparisonOperator::Eq => "IsNull",
                    ComparisonOperator::Neq => "IsNotNull",
                    _ => {
                        return Err(HqlError::new(
                            "cannot use an ordering comparison with NULL",
                            Some(column.span.clone()),
                            Some("use IS NULL / IS NOT NULL"),
                        ))
                    }
                };
                single_property(name, json!(column.source))
            } else {
                let name = match operator {
                    ComparisonOperator::Eq => "Eq",
                    ComparisonOperator::Neq => "Neq",
                    ComparisonOperator::Gt => "Gt",
                    ComparisonOperator::Gte => "Gte",
                    ComparisonOperator::Lt => "Lt",
                    ComparisonOperator::Lte => "Lte",
                };
                single_property(name, json!([column.source, literal(value)]))
            }
        }
        Condition::Between { column, low, high } => {
            json!({ "Between": [column.source, literal(low), literal(high)] })
        }
        Condition::In {
            column,
            values,
            negated,
        } => {
            let values = values.iter().map(literal).collect::<Vec<_>>();
            let inner = json!({ "IsIn": [column.source, array_literal(&values)] });
            if *negated {
                json!({ "Not": inner })
            } else {
                inner
            }
        }
        Condition::Like {
            column,
            pattern,
            negated,
        } => {
            if pattern.contains('_') {
                return Err(HqlError::new(
                    "LIKE does not support the single-character wildcard _",
                    Some(column.span.clone()),
                    Some("use % for a run of characters"),
                ));
            }
            let starts = pattern.starts_with('%');
            let ends = pattern.ends_with('%');
            let start = usize::from(starts);
            let end = pattern.len().saturating_sub(usize::from(ends));
            let core = &pattern[start..end.max(start)];
            if core.contains('%') {
                return Err(HqlError::new(
                    "LIKE only supports % at the start and/or end of the pattern",
                    Some(column.span.clone()),
                    Some("for example 'ali%', '%son' or '%li%'"),
                ));
            }
            let variant = match (starts, ends) {
                (true, true) => "Contains",
                (false, true) => "StartsWith",
                (true, false) => "EndsWith",
                (false, false) => "Eq",
            };
            let inner = single_property(variant, json!([column.source, { "String": core }]));
            if *negated {
                json!({ "Not": inner })
            } else {
                inner
            }
        }
        Condition::IsNull { column, negated } => single_property(
            if *negated { "IsNotNull" } else { "IsNull" },
            json!(column.source),
        ),
        Condition::Has(column) => json!({ "HasKey": column.source }),
    })
}

fn validate_condition(condition: Option<&Condition>) -> Result<(), HqlError> {
    if let Some(condition) = condition {
        let _ = predicate(condition)?;
        validate_nulls(condition)?;
    }
    Ok(())
}

fn validate_nulls(condition: &Condition) -> Result<(), HqlError> {
    match condition {
        Condition::And(parts) | Condition::Or(parts) => {
            for part in parts {
                validate_nulls(part)?;
            }
        }
        Condition::Not(part) => validate_nulls(part)?,
        Condition::Compare {
            column,
            value: Literal::Null,
            ..
        }
        | Condition::IsNull { column, .. }
            if column.virtual_column =>
        {
            return Err(HqlError::new(
                format!("{} is never null", column.name),
                Some(column.span.clone()),
                None,
            ));
        }
        _ => {}
    }
    Ok(())
}

fn literal(value: &Literal) -> Value {
    match value {
        Literal::String(value) => json!({ "String": value }),
        Literal::Integer(value) => json!({ "I64": value }),
        Literal::Float(value) => json!({ "F64": value }),
        Literal::Boolean(value) => json!({ "Bool": value }),
        Literal::Null => json!("Null"),
    }
}

fn array_literal(values: &[Value]) -> Value {
    if values.iter().all(|value| value.get("String").is_some()) {
        return json!({
            "StringArray": values.iter().filter_map(|value| value.get("String")).collect::<Vec<_>>()
        });
    }
    if values.iter().all(|value| value.get("I64").is_some()) {
        return json!({
            "I64Array": values.iter().filter_map(|value| value.get("I64")).collect::<Vec<_>>()
        });
    }
    json!({ "Array": values })
}

fn single_property(name: &str, value: Value) -> Value {
    let mut object = Map::new();
    object.insert(name.to_owned(), value);
    Value::Object(object)
}

fn terminal_for(entity: EntityKind) -> Value {
    if entity == EntityKind::Nodes {
        json!({ "ValueMap": null })
    } else {
        json!("EdgeProperties")
    }
}

fn validate_hops(selection: &Selection) -> Result<EntityKind, HqlError> {
    let mut entity = selection.source.entity;
    for hop in &selection.hops {
        let starts_from_nodes = matches!(
            hop.direction,
            HopDirection::Out
                | HopDirection::In
                | HopDirection::Both
                | HopDirection::OutE
                | HopDirection::InE
                | HopDirection::BothE
        );
        if starts_from_nodes && entity == EntityKind::Edges {
            return Err(HqlError::new(
                format!(
                    "TRAVERSE {} starts from a node, but the query is on edges here",
                    direction_name(hop.direction)
                ),
                Some(hop.span.clone()),
                Some("step back onto nodes first with TRAVERSE SOURCE, TARGET or OTHER"),
            ));
        }
        if !starts_from_nodes && entity == EntityKind::Nodes {
            return Err(HqlError::new(
                format!(
                    "TRAVERSE {} starts from an edge, but the query is on nodes here",
                    direction_name(hop.direction)
                ),
                Some(hop.span.clone()),
                Some("reach the edges first with TRAVERSE OUT EDGES / IN EDGES / BOTH EDGES"),
            ));
        }
        entity = if matches!(
            hop.direction,
            HopDirection::OutE | HopDirection::InE | HopDirection::BothE
        ) {
            EntityKind::Edges
        } else {
            EntityKind::Nodes
        };
    }
    Ok(entity)
}

fn direction_name(direction: HopDirection) -> &'static str {
    match direction {
        HopDirection::Out => "OUT",
        HopDirection::In => "IN",
        HopDirection::Both => "BOTH",
        HopDirection::OutE => "OUT EDGES",
        HopDirection::InE => "IN EDGES",
        HopDirection::BothE => "BOTH EDGES",
        HopDirection::FromNode => "SOURCE",
        HopDirection::ToNode => "TARGET",
        HopDirection::OtherNode => "OTHER",
    }
}

fn describe_selection(selection: &Selection) -> String {
    let mut parts = vec![format!(
        "{}{}",
        if selection.source.entity == EntityKind::Nodes {
            "nodes"
        } else {
            "edges"
        },
        selection
            .source
            .label
            .as_ref()
            .map(|label| format!(":{label}"))
            .unwrap_or_default()
    )];
    for hop in &selection.hops {
        parts.push(format!(
            "{}{}",
            direction_name(hop.direction)
                .to_ascii_lowercase()
                .replace(' ', ""),
            hop.label
                .as_ref()
                .map(|label| format!("({label})"))
                .unwrap_or_default()
        ));
    }
    parts.join(" → ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compile(source: &str) -> CompiledQuery {
        compile_source(source).unwrap()
    }

    fn wire(source: &str) -> Value {
        serde_json::from_str(&compile(source).transport_json).unwrap()
    }

    #[test]
    fn lexer_preserves_i64_and_reports_spans() {
        let tokens = tokenize("SELECT 'it''s' -- comment\n9223372036854775807").unwrap();
        assert_eq!(tokens[1].text, "it's");
        assert!(matches!(
            tokens[2].numeric,
            Some(Numeric::Integer(i64::MAX))
        ));
        let error = tokenize("SELECT *\nFROM 'unterminated").unwrap_err();
        assert_eq!(error.span.unwrap().line, 2);
    }

    #[test]
    fn parses_all_statement_families() {
        assert!(matches!(
            Parser::new("SELECT * FROM NODES").unwrap().parse(),
            Ok(Statement::Select(_))
        ));
        assert!(matches!(
            Parser::new("QUERY NODES:User LIMIT 5").unwrap().parse(),
            Ok(Statement::Graph(_))
        ));
        assert!(matches!(
            Parser::new("GRAPH").unwrap().parse(),
            Ok(Statement::Graph(_))
        ));
        assert!(matches!(
            Parser::new("SHOW STATS").unwrap().parse(),
            Ok(Statement::Show(_))
        ));
        assert!(matches!(
            Parser::new("DESCRIBE NODE 42").unwrap().parse(),
            Ok(Statement::Describe(_))
        ));
    }

    #[test]
    fn rejects_writes_and_invalid_traversals() {
        assert!(compile_source("INSERT INTO NODES VALUES (1)")
            .unwrap_err()
            .hint
            .unwrap()
            .contains("read-only"));
        assert!(compile_source("SELECT id FROM EDGES TRAVERSE OUT")
            .unwrap_err()
            .message
            .contains("starts from a node"));
        assert!(compile_source("QUERY NODES TRAVERSE OUT EDGES")
            .unwrap_err()
            .message
            .contains("must land on nodes"));
    }

    #[test]
    fn compiles_exact_large_integers_and_expected_envelope() {
        let compiled = compile("SELECT id FROM NODES WHERE externalId = 9223372036854775807");
        assert!(compiled.transport_json.contains("9223372036854775807"));
        let value = wire("SELECT * FROM NODES");
        assert_eq!(value["request_type"], "read");
        assert_eq!(value["query_name"], "helix_visualizer_select_star");
    }

    #[test]
    fn graph_limits_apply_before_edge_fanout() {
        let value = wire("QUERY LIMIT 25 EDGE LIMIT 90");
        let queries = value["query"]["queries"].as_array().unwrap();
        let edges = &queries[1]["Query"]["steps"];
        assert_eq!(edges[2], json!({ "Limit": 25 }));
        assert_eq!(edges[5], json!({ "Limit": 90 }));
    }

    #[test]
    fn validates_like_patterns_and_null_virtuals() {
        assert!(compile_source("SELECT id FROM NODES WHERE name LIKE 'a%e%'").is_err());
        assert!(compile_source("SELECT id FROM NODES WHERE id IS NULL").is_err());
        assert!(compile_source("SELECT id FROM NODES WHERE bio IS NULL").is_ok());
    }

    #[test]
    fn splash_preview_is_valid_hql() {
        let source = "QUERY NODES:User\nWHERE active = true\nTRAVERSE OUT Follows\nWITH name, email\nORDER BY name ASC\nLIMIT 150  EDGE LIMIT 600";
        let compiled = compile_source(source).unwrap();
        assert!(matches!(
            compiled.shape,
            ResultShape::Graph {
                node_limit: 150,
                edge_limit: 600,
                ..
            }
        ));
    }
}
