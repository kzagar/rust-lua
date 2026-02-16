use crate::error::LuaError;
use crate::gc::GcHeap;
use crate::value::Value;
use crate::vm::{Instruction, OpCode, Proto, UpvalDesc};
use std::collections::HashMap;

#[derive(Debug, PartialEq, Clone)]
enum Token {
    Name(String),
    Number(f64),
    Integer(i64),
    String(Vec<u8>),
    // Keywords
    Local,
    Nil,
    True,
    False,
    And,
    Or,
    Not,
    If,
    Then,
    Else,
    Elseif,
    End,
    While,
    Do,
    Repeat,
    Until,
    For,
    In,
    Function,
    Return,
    Break,
    Global,
    Goto,
    // Operators
    Plus,
    Minus,
    Mul,
    Div,
    IDiv,
    Mod,
    Pow,
    BAnd,
    BOr,
    BXor,
    Shl,
    Shr,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    Assign,
    Dot,
    Comma,
    Semi,
    Colon,
    LParen,
    RParen,
    LCurly,
    RCurly,
    LBracket,
    RBracket,
    Concat,
    Dots,
    Len,
    Eof,
}

struct Lexer<'a> {
    input: &'a [u8],
    pos: usize,
    line: usize,
}

impl<'a> Lexer<'a> {
    fn new(input: &'a [u8]) -> Self {
        let mut lexer = Self {
            input,
            pos: 0,
            line: 1,
        };
        // Skip shebang
        if lexer.peek() == Some(b'#') {
            while let Some(c) = lexer.next() {
                if c == b'\n' {
                    break;
                }
            }
        }
        lexer
    }

    fn peek(&self) -> Option<u8> {
        self.input.get(self.pos).cloned()
    }

    fn next(&mut self) -> Option<u8> {
        let res = self.peek();
        if let Some(c) = res {
            self.pos += 1;
            if c == b'\n' {
                self.line += 1;
            }
        }
        res
    }

    fn next_token(&mut self) -> Result<Token, LuaError> {
        self.skip_whitespace_and_comments();
        let c = match self.next() {
            Some(c) => c,
            None => return Ok(Token::Eof),
        };

        match c {
            b'+' => Ok(Token::Plus),
            b'*' => Ok(Token::Mul),
            b'%' => Ok(Token::Mod),
            b'^' => Ok(Token::Pow),
            b'&' => Ok(Token::BAnd),
            b'|' => Ok(Token::BOr),
            b'#' => Ok(Token::Len),
            b'(' => Ok(Token::LParen),
            b')' => Ok(Token::RParen),
            b'{' => Ok(Token::LCurly),
            b'}' => Ok(Token::RCurly),
            b'[' => {
                let mut level = 0;
                while self.peek() == Some(b'=') {
                    level += 1;
                    self.next();
                }
                if self.peek() == Some(b'[') {
                    self.next();
                    let s = self.read_long_bracket(level)?;
                    Ok(Token::String(s))
                } else if level == 0 {
                    Ok(Token::LBracket)
                } else {
                    Err(LuaError::SyntaxError("invalid long bracket".to_string()))
                }
            }
            b']' => Ok(Token::RBracket),
            b';' => Ok(Token::Semi),
            b',' => Ok(Token::Comma),
            b'-' => {
                if self.peek() == Some(b'-') {
                    self.next();
                    self.skip_comment()?;
                    self.next_token()
                } else {
                    Ok(Token::Minus)
                }
            }
            b'/' => {
                if self.peek() == Some(b'/') {
                    self.next();
                    Ok(Token::IDiv)
                } else {
                    Ok(Token::Div)
                }
            }
            b'~' => {
                if self.peek() == Some(b'=') {
                    self.next();
                    Ok(Token::Ne)
                } else {
                    Ok(Token::BXor)
                }
            }
            b'=' => {
                if self.peek() == Some(b'=') {
                    self.next();
                    Ok(Token::Eq)
                } else {
                    Ok(Token::Assign)
                }
            }
            b'<' => match self.peek() {
                Some(b'=') => {
                    self.next();
                    Ok(Token::Le)
                }
                Some(b'<') => {
                    self.next();
                    Ok(Token::Shl)
                }
                _ => Ok(Token::Lt),
            },
            b'>' => match self.peek() {
                Some(b'=') => {
                    self.next();
                    Ok(Token::Ge)
                }
                Some(b'>') => {
                    self.next();
                    Ok(Token::Shr)
                }
                _ => Ok(Token::Gt),
            },
            b'.' => {
                if self.peek() == Some(b'.') {
                    self.next();
                    if self.peek() == Some(b'.') {
                        self.next();
                        Ok(Token::Dots)
                    } else {
                        Ok(Token::Concat)
                    }
                } else {
                    Ok(Token::Dot)
                }
            }
            b':' => Ok(Token::Colon),
            b'"' | b'\'' => self.read_string(c as char),
            c if c.is_ascii_digit() => self.read_number(c as char),
            c if c.is_ascii_alphabetic() || c == b'_' || c >= 0x80 => self.read_name(c as char),
            _ => Err(LuaError::SyntaxError(format!(
                "unexpected character: {}",
                c as char
            ))),
        }
    }

    fn skip_whitespace_and_comments(&mut self) {
        loop {
            match self.peek() {
                Some(c) if (c as char).is_whitespace() => {
                    self.next();
                }
                _ => break,
            }
        }
    }

    fn skip_comment(&mut self) -> Result<(), LuaError> {
        if self.peek() == Some(b'[') {
            self.next();
            let mut level = 0;
            while self.peek() == Some(b'=') {
                level += 1;
                self.next();
            }
            if self.peek() == Some(b'[') {
                self.next();
                self.read_long_bracket(level)?;
                return Ok(());
            }
        }
        while let Some(c) = self.next() {
            if c == b'\n' {
                break;
            }
        }
        Ok(())
    }

    fn encode_utf8_extended(val: u32, s: &mut Vec<u8>) {
        if val <= 0x7F {
            s.push(val as u8);
        } else if val <= 0x7FF {
            s.push(0xC0 | (val >> 6) as u8);
            s.push(0x80 | (val & 0x3F) as u8);
        } else if val <= 0xFFFF {
            s.push(0xE0 | (val >> 12) as u8);
            s.push(0x80 | ((val >> 6) & 0x3F) as u8);
            s.push(0x80 | (val & 0x3F) as u8);
        } else if val <= 0x1FFFFF {
            s.push(0xF0 | (val >> 18) as u8);
            s.push(0x80 | ((val >> 12) & 0x3F) as u8);
            s.push(0x80 | ((val >> 6) & 0x3F) as u8);
            s.push(0x80 | (val & 0x3F) as u8);
        } else if val <= 0x3FFFFFF {
            s.push(0xF8 | (val >> 24) as u8);
            s.push(0x80 | ((val >> 18) & 0x3F) as u8);
            s.push(0x80 | ((val >> 12) & 0x3F) as u8);
            s.push(0x80 | ((val >> 6) & 0x3F) as u8);
            s.push(0x80 | (val & 0x3F) as u8);
        } else if val <= 0x7FFFFFFF {
            s.push(0xFC | (val >> 30) as u8);
            s.push(0x80 | ((val >> 24) & 0x3F) as u8);
            s.push(0x80 | ((val >> 18) & 0x3F) as u8);
            s.push(0x80 | ((val >> 12) & 0x3F) as u8);
            s.push(0x80 | ((val >> 6) & 0x3F) as u8);
            s.push(0x80 | (val & 0x3F) as u8);
        }
    }

    fn read_string(&mut self, quote: char) -> Result<Token, LuaError> {
        let mut s = Vec::new();
        let q_byte = quote as u8;
        while let Some(c) = self.next() {
            if c == q_byte {
                return Ok(Token::String(s));
            }
            if c == b'\\' {
                let next = self
                    .next()
                    .ok_or(LuaError::SyntaxError("unfinished string".to_string()))?;
                match next {
                    b'a' => s.push(0x07),
                    b'b' => s.push(0x08),
                    b'f' => s.push(0x0C),
                    b'n' => s.push(b'\n'),
                    b'r' => s.push(b'\r'),
                    b't' => s.push(b'\t'),
                    b'v' => s.push(0x0B),
                    b'\\' => s.push(b'\\'),
                    b'"' => s.push(b'"'),
                    b'\'' => s.push(b'\''),
                    b'\n' => s.push(b'\n'),
                    b'z' => {
                        while let Some(c) = self.peek() {
                            if (c as char).is_whitespace() {
                                self.next();
                            } else {
                                break;
                            }
                        }
                    }
                    b'x' => {
                        let h1 = self
                            .next()
                            .ok_or(LuaError::SyntaxError("unfinished hex escape".to_string()))?;
                        let h2 = self
                            .next()
                            .ok_or(LuaError::SyntaxError("unfinished hex escape".to_string()))?;
                        let hex = format!("{}{}", h1 as char, h2 as char);
                        let val = u8::from_str_radix(&hex, 16)
                            .map_err(|_| LuaError::SyntaxError("invalid hex escape".to_string()))?;
                        s.push(val);
                    }
                    b'u' => {
                        if self.next() != Some(b'{') {
                            return Err(LuaError::SyntaxError(
                                "expected '{' in unicode escape".to_string(),
                            ));
                        }
                        let mut hex = String::new();
                        loop {
                            let c = self.next().ok_or(LuaError::SyntaxError(
                                "unfinished unicode escape".to_string(),
                            ))?;
                            if c == b'}' {
                                break;
                            }
                            hex.push(c as char);
                        }
                        let val = u32::from_str_radix(&hex, 16).map_err(|_| {
                            LuaError::SyntaxError("invalid unicode escape".to_string())
                        })?;
                        Self::encode_utf8_extended(val, &mut s);
                    }
                    c if c.is_ascii_digit() => {
                        let mut dec = (c as char).to_string();
                        for _ in 0..2 {
                            if let Some(nc) = self.peek() {
                                if nc.is_ascii_digit() {
                                    dec.push(self.next().unwrap() as char);
                                } else {
                                    break;
                                }
                            }
                        }
                        let val = dec.parse::<u16>().map_err(|_| {
                            LuaError::SyntaxError("invalid decimal escape".to_string())
                        })?;
                        if val > 255 {
                            return Err(LuaError::SyntaxError(
                                "decimal escape too large".to_string(),
                            ));
                        }
                        s.push(val as u8);
                    }
                    _ => {
                        return Err(LuaError::SyntaxError(format!(
                            "invalid escape sequence: \\{}",
                            next as char
                        )))
                    }
                }
            } else {
                s.push(c);
            }
        }
        Err(LuaError::SyntaxError("unfinished string".to_string()))
    }

    fn read_long_bracket(&mut self, level: usize) -> Result<Vec<u8>, LuaError> {
        // Skip first newline if present
        if let Some(c) = self.peek() {
            if c == b'\n' || c == b'\r' {
                let c1 = self.next().unwrap();
                if let Some(c2) = self.peek() {
                    if (c2 == b'\n' || c2 == b'\r') && c2 != c1 {
                        self.next();
                    }
                }
            }
        }
        let mut s = Vec::new();
        loop {
            match self.next() {
                Some(b']') => {
                    let mut count = 0;
                    while self.peek() == Some(b'=') {
                        count += 1;
                        self.next();
                    }
                    if count == level && self.peek() == Some(b']') {
                        self.next();
                        return Ok(s);
                    } else {
                        s.push(b']');
                        s.extend(std::iter::repeat_n(b'=', count));
                    }
                }
                Some(b'\n') | Some(b'\r') => {
                    let c1 = self.input[self.pos - 1];
                    if let Some(c2) = self.peek() {
                        if (c2 == b'\n' || c2 == b'\r') && c2 != c1 {
                            self.next();
                        }
                    }
                    s.push(b'\n');
                }
                Some(c) => {
                    s.push(c);
                }
                None => {
                    return Err(LuaError::SyntaxError(
                        "unfinished long string/comment".to_string(),
                    ))
                }
            }
        }
    }

    fn read_number(&mut self, first: char) -> Result<Token, LuaError> {
        let mut s = first.to_string();
        if first == '0' && (self.peek() == Some(b'x') || self.peek() == Some(b'X')) {
            s.push(self.next().unwrap() as char); // x
            while let Some(c) = self.peek() {
                if c.is_ascii_hexdigit() || c == b'.' {
                    s.push(self.next().unwrap() as char);
                } else {
                    break;
                }
            }
            if s.contains('.') {
                return Err(LuaError::SyntaxError(
                    "hexadecimal floats not yet supported".to_string(),
                ));
            } else {
                let val = i64::from_str_radix(&s[2..], 16)
                    .or_else(|_| u64::from_str_radix(&s[2..], 16).map(|v| v as i64))
                    .map_err(|_| {
                        LuaError::SyntaxError("invalid hexadecimal integer".to_string())
                    })?;
                return Ok(Token::Integer(val));
            }
        }

        let mut is_float = false;
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                s.push(self.next().unwrap() as char);
            } else if c == b'.' && !is_float {
                is_float = true;
                s.push(self.next().unwrap() as char);
            } else if (c == b'e' || c == b'E') && !is_float {
                is_float = true;
                s.push(self.next().unwrap() as char);
                if self.peek() == Some(b'+') || self.peek() == Some(b'-') {
                    s.push(self.next().unwrap() as char);
                }
            } else {
                break;
            }
        }
        if is_float {
            Ok(Token::Number(s.parse().map_err(|_| {
                LuaError::SyntaxError("invalid number".to_string())
            })?))
        } else {
            Ok(Token::Integer(s.parse().map_err(|_| {
                LuaError::SyntaxError("invalid integer".to_string())
            })?))
        }
    }

    fn read_name(&mut self, first: char) -> Result<Token, LuaError> {
        let mut bytes = vec![first as u8];
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() || c == b'_' || c >= 0x80 {
                bytes.push(self.next().unwrap());
            } else {
                break;
            }
        }
        let s = String::from_utf8_lossy(&bytes).into_owned();
        match s.as_str() {
            "local" => Ok(Token::Local),
            "nil" => Ok(Token::Nil),
            "true" => Ok(Token::True),
            "false" => Ok(Token::False),
            "and" => Ok(Token::And),
            "or" => Ok(Token::Or),
            "not" => Ok(Token::Not),
            "if" => Ok(Token::If),
            "then" => Ok(Token::Then),
            "else" => Ok(Token::Else),
            "elseif" => Ok(Token::Elseif),
            "end" => Ok(Token::End),
            "while" => Ok(Token::While),
            "do" => Ok(Token::Do),
            "repeat" => Ok(Token::Repeat),
            "until" => Ok(Token::Until),
            "for" => Ok(Token::For),
            "in" => Ok(Token::In),
            "function" => Ok(Token::Function),
            "return" => Ok(Token::Return),
            "break" => Ok(Token::Break),
            "global" => Ok(Token::Global),
            "goto" => Ok(Token::Goto),
            _ => Ok(Token::Name(s)),
        }
    }
}

struct Local {
    name: String,
    depth: usize,
    reg: usize,
    is_const: bool,
    is_close: bool,
}

struct CompileState {
    locals: Vec<Local>,
    upvalues: Vec<UpvalDesc>,
    protos: Vec<crate::gc::Gc<Proto>>,
    scope_depth: usize,
    next_reg: usize,
    k: Vec<Value>,
    instructions: Vec<Instruction>,
    numparams: u8,
    is_vararg: bool,
    maxstacksize: u8,
    declared_globals: HashMap<String, bool>, // name -> is_const
    global_const_all: bool,
}

impl CompileState {
    fn new(is_main: bool) -> Self {
        let upvalues = if is_main {
            vec![UpvalDesc {
                name: "_ENV".to_string(),
                instack: true,
                idx: 0,
            }]
        } else {
            Vec::new()
        };
        Self {
            locals: Vec::new(),
            upvalues,
            protos: Vec::new(),
            scope_depth: 0,
            next_reg: 0,
            k: Vec::new(),
            instructions: Vec::new(),
            numparams: 0,
            is_vararg: false,
            maxstacksize: 2,
            declared_globals: HashMap::new(),
            global_const_all: false,
        }
    }

    fn add_k(&mut self, val: Value) -> usize {
        for (i, v) in self.k.iter().enumerate() {
            if v == &val {
                return i;
            }
        }
        self.k.push(val);
        self.k.len() - 1
    }

    fn resolve_local(&self, name: &str) -> Option<usize> {
        self.locals
            .iter()
            .rev()
            .find(|l| l.name == name)
            .map(|l| l.reg)
    }

    fn find_local(&self, name: &str) -> Option<&Local> {
        self.locals.iter().rev().find(|l| l.name == name)
    }

    fn push_reg(&mut self) -> usize {
        let r = self.next_reg;
        self.next_reg += 1;
        if self.next_reg > self.maxstacksize as usize {
            self.maxstacksize = self.next_reg as u8;
        }
        r
    }

    fn pop_regs(&mut self, n: usize) {
        self.next_reg -= n;
    }
}

pub struct Parser<'a> {
    lexer: Lexer<'a>,
    lookahead: Token,
    lookahead_line: usize,
    lookahead2: Option<Token>,
    lookahead2_line: usize,
    states: Vec<CompileState>,
    heap: &'a mut GcHeap,
}

impl<'a> Parser<'a> {
    pub fn new(input: &'a [u8], heap: &'a mut GcHeap) -> Result<Self, LuaError> {
        let mut lexer = Lexer::new(input);
        let lookahead_line = lexer.line;
        let lookahead = lexer.next_token()?;
        Ok(Self {
            lexer,
            lookahead,
            lookahead_line,
            lookahead2: None,
            lookahead2_line: 0,
            states: vec![CompileState::new(true)],
            heap,
        })
    }

    fn consume(&mut self) -> Result<Token, LuaError> {
        let old = self.lookahead.clone();
        if let Some(t2) = self.lookahead2.take() {
            self.lookahead = t2;
            self.lookahead_line = self.lookahead2_line;
        } else {
            self.lookahead_line = self.lexer.line;
            self.lookahead = self.lexer.next_token()?;
        }
        Ok(old)
    }

    fn peek(&self) -> &Token {
        &self.lookahead
    }

    fn peek2(&mut self) -> Result<&Token, LuaError> {
        if self.lookahead2.is_none() {
            self.lookahead2_line = self.lexer.line;
            self.lookahead2 = Some(self.lexer.next_token()?);
        }
        Ok(self.lookahead2.as_ref().unwrap())
    }

    fn expect(&mut self, expected: Token) -> Result<(), LuaError> {
        let line = self.lookahead_line;
        let token = self.consume()?;
        if token == expected {
            Ok(())
        } else {
            Err(LuaError::SyntaxError(format!(
                "[line {}] expected {:?}, got {:?}",
                line, expected, token
            )))
        }
    }

    fn current_state(&mut self) -> &mut CompileState {
        self.states.last_mut().unwrap()
    }

    fn add_string_k(&mut self, name: String) -> usize {
        let s_gc = self.heap.allocate(name.into_bytes());
        self.current_state().add_k(Value::String(s_gc))
    }

    fn check_const_assignment(&mut self, name: &str) -> Result<(), LuaError> {
        if let Some(local) = self.current_state().find_local(name) {
            if local.is_const {
                return Err(LuaError::SyntaxError(format!(
                    "attempt to assign to const variable '{}'",
                    name
                )));
            }
            return Ok(());
        }
        if self.is_global_const(name) {
            return Err(LuaError::SyntaxError(format!(
                "attempt to assign to const global '{}'",
                name
            )));
        }
        Ok(())
    }

    fn emit(&mut self, instr: u32) {
        let state = self.current_state();
        state.instructions.push(Instruction(instr));
    }

    pub fn parse_chunk(mut self) -> Result<Proto, LuaError> {
        while self.peek() != &Token::Eof {
            self.parse_statement()?;
        }
        // Add implicit return
        self.emit(OpCode::Return0 as u32);
        let state = self.states.pop().unwrap();
        Ok(Proto {
            instructions: state.instructions,
            k: state.k,
            upvalues: state.upvalues,
            protos: state.protos,
            numparams: state.numparams,
            is_vararg: state.is_vararg,
            maxstacksize: state.maxstacksize,
        })
    }

    fn enter_scope(&mut self) {
        self.current_state().scope_depth += 1;
    }

    fn exit_scope(&mut self) {
        let state = self.current_state();
        state.scope_depth -= 1;
        while let Some(local) = state.locals.last() {
            if local.depth > state.scope_depth {
                if local.is_close {
                    let reg = local.reg;
                    state
                        .instructions
                        .push(Instruction(OpCode::Close as u32 | ((reg as u32) << 7)));
                }
                state.locals.pop();
                state.next_reg -= 1;
            } else {
                break;
            }
        }
    }

    fn parse_statement(&mut self) -> Result<(), LuaError> {
        match self.peek().clone() {
            Token::If => self.parse_if_statement()?,
            Token::While => self.parse_while_statement()?,
            Token::Repeat => self.parse_repeat_statement()?,
            Token::For => self.parse_for_statement()?,
            Token::Do => {
                self.consume()?;
                self.enter_scope();
                while self.peek() != &Token::End && self.peek() != &Token::Eof {
                    self.parse_statement()?;
                }
                self.expect(Token::End)?;
                self.exit_scope();
            }
            Token::Break => {
                self.consume()?;
                return Err(LuaError::SyntaxError(
                    "break not yet fully supported".to_string(),
                ));
            }
            Token::Local => {
                self.consume()?;
                if self.peek() == &Token::Function {
                    self.consume()?;
                    self.parse_function_definition(true)?;
                } else {
                    self.parse_local_declaration()?;
                }
            }
            Token::Global => {
                self.consume()?;
                if self.peek() == &Token::Function {
                    self.consume()?;
                    self.parse_function_definition(false)?;
                } else {
                    self.parse_global_declaration()?;
                }
            }
            Token::Function => {
                self.consume()?;
                self.parse_function_definition(false)?;
            }
            Token::Return => {
                self.consume()?;
                self.parse_return_statement()?;
            }
            Token::Semi => {
                self.consume()?;
            }
            Token::Name(name) => {
                self.consume()?;
                self.parse_name_statement(name)?;
            }
            Token::LParen => self.parse_call_statement_with_paren()?,
            _ => {
                return Err(LuaError::SyntaxError(format!(
                    "[line {}] unexpected token in statement: {:?}",
                    self.lookahead_line,
                    self.peek()
                )))
            }
        }
        Ok(())
    }

    fn parse_name_statement(&mut self, name: String) -> Result<(), LuaError> {
        let mut names = vec![name];
        while self.peek() == &Token::Comma {
            self.consume()?;
            if let Token::Name(n) = self.consume()? {
                names.push(n);
            } else {
                return Err(LuaError::SyntaxError(
                    "expected name in variable list".to_string(),
                ));
            }
        }

        if self.peek() == &Token::Assign {
            self.consume()?;
            let start_reg = self.current_state().next_reg;
            let mut exp_count = 0;
            loop {
                let reg = self.current_state().push_reg();
                self.parse_expression(reg)?;
                exp_count += 1;
                if self.peek() == &Token::Comma {
                    self.consume()?;
                } else {
                    break;
                }
            }

            for (i, name) in names.into_iter().enumerate() {
                let src_reg = if i < exp_count {
                    start_reg + i
                } else {
                    let reg = self.current_state().push_reg();
                    self.emit(OpCode::LoadNil as u32 | ((reg as u32) << 7));
                    reg
                };
                self.check_const_assignment(&name)?;
                self.emit_store(name, src_reg)?;
            }
            self.current_state().next_reg = start_reg;
            return Ok(());
        }

        if names.len() > 1 {
            return Err(LuaError::SyntaxError(
                "expected '=' after variable list".to_string(),
            ));
        }
        let name = names.pop().unwrap();

        let reg = self.current_state().push_reg();
        self.emit_load(name, reg)?;

        loop {
            match self.peek() {
                Token::Dot => {
                    self.consume()?;
                    let field = if let Token::Name(f) = self.consume()? {
                        f
                    } else {
                        return Err(LuaError::SyntaxError("expected name".to_string()));
                    };
                    if self.peek() == &Token::Assign {
                        self.consume()?;
                        let val_reg = self.current_state().push_reg();
                        self.parse_expression(val_reg)?;
                        let k_field = self.add_string_k(field);
                        self.emit(
                            OpCode::SetField as u32
                                | ((reg as u32) << 7)
                                | ((k_field as u32) << 24)
                                | ((val_reg as u32) << 15),
                        );
                        self.current_state().pop_regs(2);
                        return Ok(());
                    } else {
                        let k_field = self.add_string_k(field);
                        self.emit(
                            OpCode::GetField as u32
                                | ((reg as u32) << 7)
                                | ((reg as u32) << 24)
                                | ((k_field as u32) << 15),
                        );
                    }
                }
                Token::Colon => {
                    self.consume()?;
                    let method = if let Token::Name(m) = self.consume()? {
                        m
                    } else {
                        return Err(LuaError::SyntaxError("expected name".to_string()));
                    };
                    let k_method = self.add_string_k(method);
                    self.emit(
                        OpCode::SelfOp as u32
                            | ((reg as u32) << 7)
                            | ((reg as u32) << 24)
                            | ((k_method as u32) << 16)
                            | (1 << 15),
                    );
                    self.parse_call_internal(reg, 0, true)?;
                    self.current_state().pop_regs(1);
                    return Ok(());
                }
                Token::LParen | Token::String(_) | Token::LCurly => {
                    self.parse_call(reg, 1)?;
                    if !matches!(self.peek(), Token::Dot | Token::LBracket | Token::Colon) {
                        self.current_state().pop_regs(1);
                        return Ok(());
                    }
                }
                Token::LBracket => {
                    self.consume()?;
                    let key_reg = self.current_state().push_reg();
                    self.parse_expression(key_reg)?;
                    self.expect(Token::RBracket)?;
                    if self.peek() == &Token::Assign {
                        self.consume()?;
                        let val_reg = self.current_state().push_reg();
                        self.parse_expression(val_reg)?;
                        self.emit(
                            OpCode::SetTable as u32
                                | ((reg as u32) << 7)
                                | ((key_reg as u32) << 24)
                                | ((val_reg as u32) << 15),
                        );
                        self.current_state().pop_regs(3);
                        return Ok(());
                    } else {
                        self.emit(
                            OpCode::GetTable as u32
                                | ((reg as u32) << 7)
                                | ((reg as u32) << 24)
                                | ((key_reg as u32) << 15),
                        );
                        self.current_state().pop_regs(1);
                    }
                }
                _ => break,
            }
        }
        self.current_state().pop_regs(1);
        Ok(())
    }

    fn parse_call_statement_with_paren(&mut self) -> Result<(), LuaError> {
        let reg = self.current_state().push_reg();
        self.parse_primary(reg)?;
        self.current_state().pop_regs(1);
        Ok(())
    }

    fn parse_if_statement(&mut self) -> Result<(), LuaError> {
        self.consume()?; // if
        let mut end_jumps = Vec::new();
        let cond_reg = self.current_state().push_reg();
        self.parse_expression(cond_reg)?;
        self.expect(Token::Then)?;
        self.emit(OpCode::Test as u32 | ((cond_reg as u32) << 7));
        let jmp_to_next = self.current_state().instructions.len();
        self.emit(OpCode::Jmp as u32);
        self.current_state().pop_regs(1);
        self.enter_scope();
        while !matches!(
            self.peek(),
            Token::End | Token::Else | Token::Elseif | Token::Eof
        ) {
            self.parse_statement()?;
        }
        self.exit_scope();
        if matches!(self.peek(), Token::Else | Token::Elseif) {
            let jmp_to_end = self.current_state().instructions.len();
            self.emit(OpCode::Jmp as u32);
            end_jumps.push(jmp_to_end);
        }
        let next_pc = self.current_state().instructions.len();
        let diff = next_pc as i32 - jmp_to_next as i32 - 1;
        self.current_state().instructions[jmp_to_next] =
            Instruction(OpCode::Jmp as u32 | (((diff + 0xFFFFFF) as u32) << 7));
        while self.peek() == &Token::Elseif {
            self.consume()?;
            let cond_reg = self.current_state().push_reg();
            self.parse_expression(cond_reg)?;
            self.expect(Token::Then)?;
            self.emit(OpCode::Test as u32 | ((cond_reg as u32) << 7));
            let jmp_to_next = self.current_state().instructions.len();
            self.emit(OpCode::Jmp as u32);
            self.current_state().pop_regs(1);
            self.enter_scope();
            while !matches!(
                self.peek(),
                Token::End | Token::Else | Token::Elseif | Token::Eof
            ) {
                self.parse_statement()?;
            }
            self.exit_scope();
            if matches!(self.peek(), Token::Else | Token::Elseif) {
                let jmp_to_end = self.current_state().instructions.len();
                self.emit(OpCode::Jmp as u32);
                end_jumps.push(jmp_to_end);
            }
            let next_pc = self.current_state().instructions.len();
            let diff = next_pc as i32 - jmp_to_next as i32 - 1;
            self.current_state().instructions[jmp_to_next] =
                Instruction(OpCode::Jmp as u32 | (((diff + 0xFFFFFF) as u32) << 7));
        }
        if self.peek() == &Token::Else {
            self.consume()?;
            self.enter_scope();
            while self.peek() != &Token::End && self.peek() != &Token::Eof {
                self.parse_statement()?;
            }
            self.exit_scope();
        }
        self.expect(Token::End)?;
        let end_pc = self.current_state().instructions.len();
        for jmp_idx in end_jumps {
            let diff = end_pc as i32 - jmp_idx as i32 - 1;
            self.current_state().instructions[jmp_idx] =
                Instruction(OpCode::Jmp as u32 | (((diff + 0xFFFFFF) as u32) << 7));
        }
        Ok(())
    }

    fn parse_while_statement(&mut self) -> Result<(), LuaError> {
        self.consume()?; // while
        let start_pc = self.current_state().instructions.len();
        let cond_reg = self.current_state().push_reg();
        self.parse_expression(cond_reg)?;
        self.expect(Token::Do)?;
        self.emit(OpCode::Test as u32 | ((cond_reg as u32) << 7));
        let jmp_to_end = self.current_state().instructions.len();
        self.emit(OpCode::Jmp as u32);
        self.current_state().pop_regs(1);
        self.enter_scope();
        while self.peek() != &Token::End && self.peek() != &Token::Eof {
            self.parse_statement()?;
        }
        self.exit_scope();
        self.expect(Token::End)?;
        let end_pc = self.current_state().instructions.len();
        let diff = start_pc as i32 - end_pc as i32 - 1;
        self.emit(OpCode::Jmp as u32 | (((diff + 0xFFFFFF) as u32) << 7));
        let final_pc = self.current_state().instructions.len();
        let diff_end = final_pc as i32 - jmp_to_end as i32 - 1;
        self.current_state().instructions[jmp_to_end] =
            Instruction(OpCode::Jmp as u32 | (((diff_end + 0xFFFFFF) as u32) << 7));
        Ok(())
    }

    fn parse_repeat_statement(&mut self) -> Result<(), LuaError> {
        self.consume()?; // repeat
        let start_pc = self.current_state().instructions.len();
        self.enter_scope();
        while self.peek() != &Token::Until && self.peek() != &Token::Eof {
            self.parse_statement()?;
        }
        self.expect(Token::Until)?;
        let cond_reg = self.current_state().push_reg();
        self.parse_expression(cond_reg)?;
        self.exit_scope();
        self.emit(OpCode::Test as u32 | ((cond_reg as u32) << 7));
        let end_pc = self.current_state().instructions.len();
        let diff = start_pc as i32 - end_pc as i32 - 1;
        self.emit(OpCode::Jmp as u32 | (((diff + 0xFFFFFF) as u32) << 7));
        self.current_state().pop_regs(1);
        Ok(())
    }

    fn parse_for_statement(&mut self) -> Result<(), LuaError> {
        self.consume()?; // for
        let mut names = Vec::new();
        if let Token::Name(name) = self.consume()? {
            names.push(name);
        } else {
            return Err(LuaError::SyntaxError("expected name".to_string()));
        }
        while self.peek() == &Token::Comma {
            self.consume()?;
            if let Token::Name(name) = self.consume()? {
                names.push(name);
            } else {
                return Err(LuaError::SyntaxError(
                    "expected name after comma".to_string(),
                ));
            }
        }
        if self.peek() == &Token::Assign {
            if names.len() != 1 {
                return Err(LuaError::SyntaxError(
                    "numeric for must have exactly one variable".to_string(),
                ));
            }
            let name = names.pop().unwrap();
            self.consume()?; // =
            let base_reg = self.current_state().next_reg;
            let init_reg = self.current_state().push_reg();
            self.parse_expression(init_reg)?;
            self.expect(Token::Comma)?;
            let limit_reg = self.current_state().push_reg();
            self.parse_expression(limit_reg)?;
            if self.peek() == &Token::Comma {
                self.consume()?;
                let step_reg = self.current_state().push_reg();
                self.parse_expression(step_reg)?;
            } else {
                let step_reg = self.current_state().push_reg();
                self.emit(OpCode::LoadI as u32 | ((step_reg as u32) << 7) | ((1 + 0xFFFF) << 15));
            }
            self.expect(Token::Do)?;
            let prep_idx = self.current_state().instructions.len();
            self.emit(OpCode::ForPrep as u32 | ((base_reg as u32) << 7));
            self.enter_scope();
            let depth = self.current_state().scope_depth;
            self.current_state().locals.push(Local {
                name,
                depth,
                reg: base_reg + 3,
                is_const: false,
                is_close: false,
            });
            self.current_state().next_reg += 1;
            while self.peek() != &Token::End && self.peek() != &Token::Eof {
                self.parse_statement()?;
            }
            self.exit_scope();
            self.expect(Token::End)?;
            let loop_idx = self.current_state().instructions.len();
            let diff = loop_idx as i32 - prep_idx as i32 - 1;
            self.current_state().instructions[prep_idx] = Instruction(
                OpCode::ForPrep as u32
                    | ((base_reg as u32) << 7)
                    | (((diff + 0xFFFF) as u32) << 15),
            );
            let back_diff = prep_idx as i32 - loop_idx as i32 - 1;
            self.emit(
                OpCode::ForLoop as u32
                    | ((base_reg as u32) << 7)
                    | (((back_diff + 0xFFFF) as u32) << 15),
            );
            self.current_state().pop_regs(3);
            Ok(())
        } else if self.peek() == &Token::In {
            self.consume()?; // in
            let base_reg = self.current_state().next_reg;
            self.current_state().push_reg();
            self.current_state().push_reg();
            self.current_state().push_reg();
            let mut nexp = 0;
            loop {
                let reg = base_reg + nexp;
                if nexp < 3 {
                    self.parse_expression(reg)?;
                } else {
                    let dummy_reg = self.current_state().push_reg();
                    self.parse_expression(dummy_reg)?;
                    self.current_state().pop_regs(1);
                }
                nexp += 1;
                if self.peek() == &Token::Comma {
                    self.consume()?;
                } else {
                    break;
                }
            }
            if nexp < 3 {
                for i in nexp..3 {
                    self.emit(OpCode::LoadNil as u32 | (((base_reg + i) as u32) << 7));
                }
            }
            self.expect(Token::Do)?;
            let prep_idx = self.current_state().instructions.len();
            self.emit(OpCode::TForPrep as u32);
            self.enter_scope();
            let nvars = names.len();
            for name in names {
                let reg = self.current_state().push_reg();
                let depth = self.current_state().scope_depth;
                self.current_state().locals.push(Local {
                    name,
                    depth,
                    reg,
                    is_const: false,
                    is_close: false,
                });
            }
            while self.peek() != &Token::End && self.peek() != &Token::Eof {
                self.parse_statement()?;
            }
            self.exit_scope();
            self.expect(Token::End)?;
            let call_idx = self.current_state().instructions.len();
            self.emit(OpCode::TForCall as u32 | ((base_reg as u32) << 7) | ((nvars as u32) << 15));
            let diff = call_idx as i32 - prep_idx as i32;
            self.emit(
                OpCode::TForLoop as u32
                    | ((base_reg as u32) << 7)
                    | ((((-diff) + 0xFFFF) as u32) << 15),
            );
            let final_pc = self.current_state().instructions.len();
            let prep_diff = final_pc as i32 - prep_idx as i32 - 1;
            self.current_state().instructions[prep_idx] =
                Instruction(OpCode::TForPrep as u32 | (((prep_diff + 0xFFFFFF) as u32) << 7));
            self.current_state().pop_regs(3);
            Ok(())
        } else {
            Err(LuaError::SyntaxError(
                "expected '=' or 'in' in for loop".to_string(),
            ))
        }
    }

    fn parse_table_constructor(&mut self, dest_reg: usize) -> Result<(), LuaError> {
        self.expect(Token::LCurly)?;
        self.emit(OpCode::NewTable as u32 | ((dest_reg as u32) << 7));
        let mut array_count = 0;
        while self.peek() != &Token::RCurly {
            match self.peek().clone() {
                Token::LBracket => {
                    self.consume()?;
                    let key_reg = self.current_state().push_reg();
                    self.parse_expression(key_reg)?;
                    self.expect(Token::RBracket)?;
                    self.expect(Token::Assign)?;
                    let val_reg = self.current_state().push_reg();
                    self.parse_expression(val_reg)?;
                    self.emit(
                        OpCode::SetTable as u32
                            | ((dest_reg as u32) << 7)
                            | ((key_reg as u32) << 24)
                            | ((val_reg as u32) << 15),
                    );
                    self.current_state().pop_regs(2);
                }
                Token::Name(name) => {
                    if self.peek2()? == &Token::Assign {
                        self.consume()?; // name
                        self.expect(Token::Assign)?;
                        let val_reg = self.current_state().push_reg();
                        self.parse_expression(val_reg)?;
                        let k_name = self.add_string_k(name);
                        self.emit(
                            OpCode::SetField as u32
                                | ((dest_reg as u32) << 7)
                                | ((k_name as u32) << 24)
                                | ((val_reg as u32) << 15),
                        );
                        self.current_state().pop_regs(1);
                    } else {
                        let val_reg = self.current_state().push_reg();
                        self.parse_expression(val_reg)?;
                        array_count += 1;
                        self.emit(
                            OpCode::SetI as u32
                                | ((dest_reg as u32) << 7)
                                | ((array_count as u32) << 24)
                                | ((val_reg as u32) << 15),
                        );
                        self.current_state().pop_regs(1);
                    }
                }
                _ => {
                    let val_reg = self.current_state().push_reg();
                    self.parse_expression(val_reg)?;
                    array_count += 1;
                    self.emit(
                        OpCode::SetI as u32
                            | ((dest_reg as u32) << 7)
                            | ((array_count as u32) << 24)
                            | ((val_reg as u32) << 15),
                    );
                    self.current_state().pop_regs(1);
                }
            }
            if self.peek() == &Token::Comma || self.peek() == &Token::Semi {
                self.consume()?;
            } else {
                break;
            }
        }
        self.expect(Token::RCurly)?;
        Ok(())
    }

    fn parse_attribute(&mut self) -> Result<String, LuaError> {
        if self.peek() == &Token::Lt {
            self.consume()?;
            let attr = if let Token::Name(name) = self.consume()? {
                name
            } else {
                return Err(LuaError::SyntaxError("expected attribute name".to_string()));
            };
            self.expect(Token::Gt)?;
            Ok(attr)
        } else {
            Ok(String::new())
        }
    }

    fn is_global_const(&self, name: &str) -> bool {
        let state = self.states.last().unwrap();
        if let Some(&is_const) = state.declared_globals.get(name) {
            return is_const;
        }
        state.global_const_all
    }

    fn parse_global_declaration(&mut self) -> Result<(), LuaError> {
        let def_attr = self.parse_attribute()?;
        let is_const_all = def_attr == "const";
        if self.peek() == &Token::Mul {
            self.consume()?;
            self.current_state().global_const_all = is_const_all;
            Ok(())
        } else {
            loop {
                if let Token::Name(name) = self.consume()? {
                    let attr = self.parse_attribute()?;
                    let is_const = attr == "const" || (is_const_all && attr.is_empty());
                    self.current_state().declared_globals.insert(name, is_const);
                } else {
                    return Err(LuaError::SyntaxError(
                        "expected name in global declaration".to_string(),
                    ));
                }
                if self.peek() == &Token::Comma {
                    self.consume()?;
                } else {
                    break;
                }
            }
            Ok(())
        }
    }

    fn parse_local_declaration(&mut self) -> Result<(), LuaError> {
        let mut names = Vec::new();
        let mut attributes = Vec::new();
        if let Token::Name(name) = self.consume()? {
            let attr = self.parse_attribute()?;
            names.push(name);
            attributes.push(attr);
            while self.peek() == &Token::Comma {
                self.consume()?;
                if let Token::Name(name) = self.consume()? {
                    let attr = self.parse_attribute()?;
                    names.push(name);
                    attributes.push(attr);
                } else {
                    return Err(LuaError::SyntaxError(
                        "expected name after comma in local declaration".to_string(),
                    ));
                }
            }
            if self.peek() == &Token::Assign {
                self.consume()?;
                let start_reg = self.current_state().next_reg;
                let mut exp_count = 0;
                loop {
                    let reg = self.current_state().push_reg();
                    self.parse_expression(reg)?;
                    exp_count += 1;
                    if self.peek() == &Token::Comma {
                        self.consume()?;
                    } else {
                        break;
                    }
                }
                for (i, (name, attr)) in names.into_iter().zip(attributes.into_iter()).enumerate() {
                    let is_const = attr == "const";
                    let is_close = attr == "close";
                    let reg = start_reg + i;
                    if i >= exp_count {
                        self.current_state().push_reg();
                        self.emit(OpCode::LoadNil as u32 | ((reg as u32) << 7));
                    }
                    if is_close {
                        self.emit(OpCode::Tbc as u32 | ((reg as u32) << 7));
                    }
                    let state = self.current_state();
                    state.locals.push(Local {
                        name,
                        depth: state.scope_depth,
                        reg,
                        is_const,
                        is_close,
                    });
                }
            } else {
                for (name, attr) in names.into_iter().zip(attributes.into_iter()) {
                    let is_const = attr == "const";
                    let is_close = attr == "close";
                    let reg = self.current_state().push_reg();
                    self.emit(OpCode::LoadNil as u32 | ((reg as u32) << 7));
                    if is_close {
                        self.emit(OpCode::Tbc as u32 | ((reg as u32) << 7));
                    }
                    let state = self.current_state();
                    state.locals.push(Local {
                        name,
                        depth: state.scope_depth,
                        reg,
                        is_const,
                        is_close,
                    });
                }
            }
            Ok(())
        } else {
            Err(LuaError::SyntaxError(
                "expected name in local declaration".to_string(),
            ))
        }
    }

    fn emit_store(&mut self, name: String, src_reg: usize) -> Result<(), LuaError> {
        if let Some(reg) = self.current_state().resolve_local(&name) {
            self.emit(OpCode::Move as u32 | ((reg as u32) << 7) | ((src_reg as u32) << 24));
        } else if let Some(uv_idx) = self.resolve_upvalue(&name) {
            self.emit(OpCode::SetUpval as u32 | ((src_reg as u32) << 7) | ((uv_idx as u32) << 15));
        } else {
            let env_idx = self.resolve_upvalue("_ENV").unwrap_or(0);
            let k_name = self.add_string_k(name);
            self.emit(
                OpCode::SetTabUp as u32
                    | ((env_idx as u32) << 7)
                    | ((k_name as u32) << 24)
                    | ((src_reg as u32) << 16)
                    | (1 << 15),
            );
        }
        Ok(())
    }

    fn resolve_upvalue(&mut self, name: &str) -> Option<usize> {
        if self.states.len() <= 1 {
            return None;
        }
        for (i, uv) in self.states.last().unwrap().upvalues.iter().enumerate() {
            if uv.name == name {
                return Some(i);
            }
        }
        let mut depth = self.states.len() - 2;
        loop {
            if let Some(reg) = self.states[depth].resolve_local(name) {
                let mut prev_uv_idx = reg;
                let mut instack = true;
                for d in (depth + 1)..self.states.len() {
                    let uv_idx = self.states[d].upvalues.len();
                    self.states[d].upvalues.push(UpvalDesc {
                        name: name.to_string(),
                        instack,
                        idx: prev_uv_idx as u8,
                    });
                    prev_uv_idx = uv_idx;
                    instack = false;
                }
                return Some(prev_uv_idx);
            }
            for (i, uv) in self.states[depth].upvalues.iter().enumerate() {
                if uv.name == name {
                    let mut prev_uv_idx = i;
                    for d in (depth + 1)..self.states.len() {
                        let uv_idx = self.states[d].upvalues.len();
                        self.states[d].upvalues.push(UpvalDesc {
                            name: name.to_string(),
                            instack: false,
                            idx: prev_uv_idx as u8,
                        });
                        prev_uv_idx = uv_idx;
                    }
                    return Some(prev_uv_idx);
                }
            }
            if depth == 0 {
                break;
            }
            depth -= 1;
        }
        None
    }

    fn parse_call(&mut self, func_reg: usize, nresults: i32) -> Result<(), LuaError> {
        self.parse_call_internal(func_reg, nresults, false)
    }

    fn parse_call_internal(
        &mut self,
        func_reg: usize,
        nresults: i32,
        has_self: bool,
    ) -> Result<(), LuaError> {
        let b = match self.peek().clone() {
            Token::LParen => {
                self.consume()?;
                if has_self {
                    self.current_state().push_reg();
                }
                let mut arg_count = if has_self { 1 } else { 0 };
                let mut vararg_call = false;
                if self.peek() != &Token::RParen {
                    loop {
                        let arg_reg = self.current_state().push_reg();
                        if self.peek() == &Token::Dots {
                            self.consume()?;
                            self.emit(OpCode::VarArg as u32 | ((arg_reg as u32) << 7));
                            vararg_call = true;
                            arg_count += 1;
                            break;
                        }
                        self.parse_expression(arg_reg)?;
                        arg_count += 1;
                        if self.peek() == &Token::Comma {
                            self.consume()?;
                        } else {
                            break;
                        }
                    }
                }
                self.expect(Token::RParen)?;
                let b = if vararg_call { 0 } else { arg_count + 1 };
                self.current_state().pop_regs(arg_count);
                b
            }
            Token::String(s) => {
                self.consume()?;
                if has_self {
                    return Err(LuaError::SyntaxError(
                        "colon call with string literal not supported".to_string(),
                    ));
                }
                let arg_reg = self.current_state().push_reg();
                let s_gc = self.heap.allocate(s);
                let k = self.current_state().add_k(Value::String(s_gc));
                self.emit(OpCode::LoadK as u32 | ((arg_reg as u32) << 7) | ((k as u32) << 15));
                self.current_state().pop_regs(1);
                2
            }
            Token::LCurly => {
                if has_self {
                    return Err(LuaError::SyntaxError(
                        "colon call with table literal not supported".to_string(),
                    ));
                }
                let arg_reg = self.current_state().push_reg();
                self.parse_table_constructor(arg_reg)?;
                self.current_state().pop_regs(1);
                2
            }
            _ => {
                return Err(LuaError::SyntaxError(format!(
                    "expected function arguments, got {:?}",
                    self.peek()
                )))
            }
        };
        let c = (nresults + 1) as u32;
        self.emit(OpCode::Call as u32 | ((func_reg as u32) << 7) | ((b as u32) << 24) | (c << 15));
        Ok(())
    }

    fn emit_load(&mut self, name: String, dest_reg: usize) -> Result<(), LuaError> {
        if let Some(reg) = self.current_state().resolve_local(&name) {
            self.emit(OpCode::Move as u32 | ((dest_reg as u32) << 7) | ((reg as u32) << 24));
        } else if let Some(uv_idx) = self.resolve_upvalue(&name) {
            self.emit(OpCode::GetUpval as u32 | ((dest_reg as u32) << 7) | ((uv_idx as u32) << 15));
        } else {
            let env_idx = self.resolve_upvalue("_ENV").unwrap_or(0);
            let k_name = self.add_string_k(name);
            self.emit(
                OpCode::GetTabUp as u32
                    | ((dest_reg as u32) << 7)
                    | ((env_idx as u32) << 24)
                    | ((k_name as u32) << 16)
                    | (1 << 15),
            );
        }
        Ok(())
    }

    fn parse_expression(&mut self, dest_reg: usize) -> Result<(), LuaError> {
        self.parse_binop(dest_reg, 0)
    }

    fn parse_return_statement(&mut self) -> Result<(), LuaError> {
        let mut nres = 0;
        let start_reg = self.current_state().next_reg;
        if !matches!(
            self.peek(),
            Token::End | Token::Eof | Token::Else | Token::Elseif | Token::Until
        ) {
            loop {
                let reg = self.current_state().push_reg();
                self.parse_expression(reg)?;
                nres += 1;
                if self.peek() == &Token::Comma {
                    self.consume()?;
                } else {
                    break;
                }
            }
        }
        self.emit(OpCode::Return as u32 | ((start_reg as u32) << 7) | (((nres + 1) as u32) << 24));
        self.current_state().pop_regs(nres);
        Ok(())
    }

    fn parse_function_definition(&mut self, is_local: bool) -> Result<(), LuaError> {
        let mut name_parts = Vec::new();
        let mut is_method = false;
        if !is_local && self.peek() != &Token::LParen {
            if let Token::Name(name) = self.consume()? {
                name_parts.push(name);
                while self.peek() == &Token::Dot {
                    self.consume()?;
                    if let Token::Name(name) = self.consume()? {
                        name_parts.push(name);
                    }
                }
                if self.peek() == &Token::Colon {
                    self.consume()?;
                    if let Token::Name(name) = self.consume()? {
                        name_parts.push(name);
                        is_method = true;
                    }
                }
            }
        } else if is_local {
            if let Token::Name(name) = self.consume()? {
                name_parts.push(name);
            }
        }
        self.states.push(CompileState::new(false));
        let mut numparams = 0;
        if is_method {
            let reg = self.current_state().push_reg();
            self.current_state().locals.push(Local {
                name: "self".to_string(),
                depth: 0,
                reg,
                is_const: false,
                is_close: false,
            });
            numparams = 1;
        }
        self.expect(Token::LParen)?;
        let mut is_vararg = false;
        if self.peek() != &Token::RParen {
            loop {
                if self.peek() == &Token::Dots {
                    self.consume()?;
                    is_vararg = true;
                    if let Token::Name(_) = self.peek() {
                        self.consume()?;
                    }
                    break;
                }
                if let Token::Name(arg_name) = self.consume()? {
                    let reg = self.current_state().push_reg();
                    self.current_state().locals.push(Local {
                        name: arg_name,
                        depth: 0,
                        reg,
                        is_const: false,
                        is_close: false,
                    });
                    numparams += 1;
                }
                if self.peek() == &Token::Comma {
                    self.consume()?;
                } else {
                    break;
                }
            }
        }
        self.expect(Token::RParen)?;
        self.current_state().numparams = numparams;
        self.current_state().is_vararg = is_vararg;
        if is_vararg {
            self.emit(OpCode::VarArgPrep as u32 | ((numparams as u32) << 7));
        }
        while self.peek() != &Token::End && self.peek() != &Token::Eof {
            self.parse_statement()?;
        }
        self.expect(Token::End)?;
        self.emit(OpCode::Return0 as u32);
        let state = self.states.pop().unwrap();
        let proto = Proto {
            instructions: state.instructions,
            k: state.k,
            upvalues: state.upvalues,
            protos: state.protos,
            numparams: state.numparams,
            is_vararg: state.is_vararg,
            maxstacksize: state.maxstacksize,
        };
        let proto_gc = self.heap.allocate(proto);
        let parent_state = self.current_state();
        let proto_idx = parent_state.protos.len();
        parent_state.protos.push(proto_gc);
        let dest_reg = parent_state.push_reg();
        self.emit(OpCode::Closure as u32 | ((dest_reg as u32) << 7) | ((proto_idx as u32) << 15));
        if is_local {
            let state = self.current_state();
            state.locals.push(Local {
                name: name_parts[0].clone(),
                depth: state.scope_depth,
                reg: dest_reg,
                is_const: false,
                is_close: false,
            });
        } else if name_parts.len() == 1 {
            self.emit_store(name_parts[0].clone(), dest_reg)?;
            self.current_state().pop_regs(1);
        } else if name_parts.len() > 1 {
            let table_reg = self.current_state().push_reg();
            self.emit_load(name_parts[0].clone(), table_reg)?;
            let k_field = self.add_string_k(name_parts[1].clone());
            self.emit(
                OpCode::SetField as u32
                    | ((table_reg as u32) << 7)
                    | ((k_field as u32) << 24)
                    | ((dest_reg as u32) << 15),
            );
            self.current_state().pop_regs(2);
        }
        Ok(())
    }

    fn get_precedence(token: &Token) -> i32 {
        match token {
            Token::Or => 1,
            Token::And => 2,
            Token::Eq | Token::Ne | Token::Lt | Token::Gt | Token::Le | Token::Ge => 3,
            Token::BOr => 4,
            Token::BXor => 5,
            Token::BAnd => 6,
            Token::Shl | Token::Shr => 7,
            Token::Concat => 8,
            Token::Plus | Token::Minus => 9,
            Token::Mul | Token::Div | Token::IDiv | Token::Mod => 10,
            Token::Not | Token::Len => 11,
            Token::Pow => 12,
            _ => 0,
        }
    }

    fn parse_binop(&mut self, dest_reg: usize, min_prec: i32) -> Result<(), LuaError> {
        self.parse_unary(dest_reg)?;
        loop {
            let token = self.peek().clone();
            let prec = Self::get_precedence(&token);
            if prec <= min_prec {
                break;
            }
            self.consume()?;
            if token == Token::And || token == Token::Or {
                self.parse_logical_op(token, dest_reg, prec)?;
            } else {
                let right_reg = self.current_state().push_reg();
                let next_min_prec = if prec == 12 || prec == 8 {
                    prec - 1
                } else {
                    prec
                };
                self.parse_binop(right_reg, next_min_prec)?;
                self.emit_binop(token, dest_reg, dest_reg, right_reg)?;
                self.current_state().pop_regs(1);
            }
        }
        Ok(())
    }

    fn parse_logical_op(&mut self, op: Token, dest_reg: usize, prec: i32) -> Result<(), LuaError> {
        let is_and = op == Token::And;
        let k = if is_and { 0 } else { 1 };
        self.emit(
            OpCode::TestSet as u32
                | ((dest_reg as u32) << 7)
                | ((dest_reg as u32) << 24)
                | (k << 15),
        );
        let jmp_idx = self.current_state().instructions.len();
        self.emit(OpCode::Jmp as u32);
        self.parse_binop(dest_reg, prec)?;
        let end_idx = self.current_state().instructions.len();
        let diff = end_idx as i32 - jmp_idx as i32 - 1;
        self.current_state().instructions[jmp_idx] =
            Instruction(OpCode::Jmp as u32 | (((diff + 0xFFFFFF) as u32) << 7));
        Ok(())
    }

    fn parse_unary(&mut self, dest_reg: usize) -> Result<(), LuaError> {
        let token = self.peek().clone();
        if token == Token::Not
            || token == Token::Len
            || token == Token::Minus
            || token == Token::BXor
        {
            self.consume()?;
            self.parse_unary(dest_reg)?;
            match token {
                Token::Minus => self.emit(
                    OpCode::Unm as u32 | ((dest_reg as u32) << 7) | ((dest_reg as u32) << 24),
                ),
                Token::Not => self.emit(
                    OpCode::Not as u32 | ((dest_reg as u32) << 7) | ((dest_reg as u32) << 24),
                ),
                Token::Len => self.emit(
                    OpCode::Len as u32 | ((dest_reg as u32) << 7) | ((dest_reg as u32) << 24),
                ),
                Token::BXor => self.emit(
                    OpCode::BNot as u32 | ((dest_reg as u32) << 7) | ((dest_reg as u32) << 24),
                ),
                _ => unreachable!(),
            }
        } else {
            self.parse_primary(dest_reg)?;
        }
        Ok(())
    }

    fn parse_primary(&mut self, dest_reg: usize) -> Result<(), LuaError> {
        match self.peek().clone() {
            Token::LCurly => {
                self.parse_table_constructor(dest_reg)?;
                self.parse_primary_suffix(dest_reg)?;
            }
            Token::Integer(i) => {
                self.consume()?;
                if (-32768..=32767).contains(&i) {
                    let val = (i + 0xFFFF) as u32;
                    self.emit(OpCode::LoadI as u32 | ((dest_reg as u32) << 7) | (val << 15));
                } else {
                    let k = self.current_state().add_k(Value::Integer(i));
                    self.emit(OpCode::LoadK as u32 | ((dest_reg as u32) << 7) | ((k as u32) << 15));
                }
            }
            Token::Number(n) => {
                self.consume()?;
                let k = self.current_state().add_k(Value::Number(n));
                self.emit(OpCode::LoadK as u32 | ((dest_reg as u32) << 7) | ((k as u32) << 15));
            }
            Token::String(s) => {
                self.consume()?;
                let s_gc = self.heap.allocate(s);
                let k = self.current_state().add_k(Value::String(s_gc));
                self.emit(OpCode::LoadK as u32 | ((dest_reg as u32) << 7) | ((k as u32) << 15));
            }
            Token::True => {
                self.consume()?;
                self.emit(OpCode::LoadTrue as u32 | ((dest_reg as u32) << 7) | (1 << 24));
            }
            Token::False => {
                self.consume()?;
                self.emit(OpCode::LoadFalse as u32 | ((dest_reg as u32) << 7));
            }
            Token::Nil => {
                self.consume()?;
                self.emit(OpCode::LoadNil as u32 | ((dest_reg as u32) << 7));
            }
            Token::Name(name) => {
                self.consume()?;
                self.emit_load(name, dest_reg)?;
                self.parse_primary_suffix(dest_reg)?;
            }
            Token::Dots => {
                self.consume()?;
                self.emit(OpCode::VarArg as u32 | ((dest_reg as u32) << 7) | (2 << 24));
            }
            Token::Function => {
                self.consume()?;
                self.parse_function_definition(false)?;
                let last_reg = self.current_state().next_reg - 1;
                if last_reg != dest_reg {
                    self.emit(
                        OpCode::Move as u32 | ((dest_reg as u32) << 7) | ((last_reg as u32) << 24),
                    );
                    self.current_state().pop_regs(1);
                }
            }
            Token::LParen => {
                self.consume()?;
                self.parse_expression(dest_reg)?;
                self.expect(Token::RParen)?;
                self.parse_primary_suffix(dest_reg)?;
            }
            _ => {
                return Err(LuaError::SyntaxError(format!(
                    "[line {}] expected expression, got {:?}",
                    self.lookahead_line,
                    self.peek()
                )))
            }
        }
        Ok(())
    }

    fn parse_primary_suffix(&mut self, dest_reg: usize) -> Result<(), LuaError> {
        loop {
            match self.peek() {
                Token::LParen | Token::String(_) | Token::LCurly => {
                    self.parse_call(dest_reg, 1)?;
                }
                Token::Dot => {
                    self.consume()?;
                    if let Token::Name(field) = self.consume()? {
                        let k_field = self.add_string_k(field);
                        self.emit(
                            OpCode::GetField as u32
                                | ((dest_reg as u32) << 7)
                                | ((dest_reg as u32) << 24)
                                | ((k_field as u32) << 15),
                        );
                    } else {
                        return Err(LuaError::SyntaxError("expected name after dot".to_string()));
                    }
                }
                Token::LBracket => {
                    self.consume()?;
                    let key_reg = self.current_state().push_reg();
                    self.parse_expression(key_reg)?;
                    self.expect(Token::RBracket)?;
                    self.emit(
                        OpCode::GetTable as u32
                            | ((dest_reg as u32) << 7)
                            | ((dest_reg as u32) << 24)
                            | ((key_reg as u32) << 15),
                    );
                    self.current_state().pop_regs(1);
                }
                Token::Colon => {
                    self.consume()?;
                    if let Token::Name(method) = self.consume()? {
                        let k_method = self.add_string_k(method);
                        self.emit(
                            OpCode::SelfOp as u32
                                | ((dest_reg as u32) << 7)
                                | ((dest_reg as u32) << 24)
                                | ((k_method as u32) << 16)
                                | (1 << 15),
                        );
                        self.parse_call_internal(dest_reg, 1, true)?;
                    } else {
                        return Err(LuaError::SyntaxError(
                            "expected name after colon".to_string(),
                        ));
                    }
                }
                _ => break,
            }
        }
        Ok(())
    }

    fn emit_binop(
        &mut self,
        op: Token,
        dest: usize,
        left: usize,
        right: usize,
    ) -> Result<(), LuaError> {
        let opcode = match op {
            Token::Plus => OpCode::Add as u32,
            Token::Minus => OpCode::Sub as u32,
            Token::Mul => OpCode::Mul as u32,
            Token::Mod => OpCode::Mod as u32,
            Token::Pow => OpCode::Pow as u32,
            Token::Div => OpCode::Div as u32,
            Token::IDiv => OpCode::IDiv as u32,
            Token::BAnd => OpCode::BAnd as u32,
            Token::BOr => OpCode::BOr as u32,
            Token::BXor => OpCode::BXor as u32,
            Token::Shl => OpCode::Shl as u32,
            Token::Shr => OpCode::Shr as u32,
            Token::Concat => OpCode::Concat as u32,
            Token::Eq => OpCode::Eq as u32,
            Token::Ne => OpCode::Eq as u32,
            Token::Lt => OpCode::Lt as u32,
            Token::Gt => OpCode::Lt as u32,
            Token::Le => OpCode::Le as u32,
            Token::Ge => OpCode::Le as u32,
            _ => {
                return Err(LuaError::SyntaxError(format!(
                    "operator {:?} not yet fully supported in expressions",
                    op
                )))
            }
        };
        let d = dest as u32;
        let l = left as u32;
        let r = right as u32;
        if op == Token::Ne {
            self.emit(opcode | (l << 7) | (r << 24) | (1 << 15));
        } else if op == Token::Gt || op == Token::Ge {
            self.emit(opcode | (r << 7) | (l << 24));
        } else if op == Token::Eq || op == Token::Lt || op == Token::Le {
            self.emit(opcode | (l << 7) | (r << 24));
        } else {
            self.emit(opcode | (d << 7) | (l << 24) | (r << 16));
        }
        Ok(())
    }
}
