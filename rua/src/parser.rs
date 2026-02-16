use crate::error::LuaError;
use crate::vm::{Proto, Instruction, UpvalDesc, OpCode};
use crate::value::Value;
use crate::gc::GcHeap;
use std::collections::HashMap;

#[derive(Debug, PartialEq, Clone)]
enum Token {
    Name(String),
    Number(f64),
    Integer(i64),
    String(Vec<u8>),
    // Keywords
    Local, Nil, True, False, And, Or, Not, If, Then, Else, Elseif, End,
    While, Do, Repeat, Until, For, In, Function, Return, Break, Global, Goto,
    // Operators
    Plus, Minus, Mul, Div, IDiv, Mod, Pow,
    BAnd, BOr, BXor, Shl, Shr,
    Eq, Ne, Lt, Gt, Le, Ge,
    Assign, Dot, Comma, Semi, Colon,
    LParen, RParen, LCurly, RCurly, LBracket, RBracket,
    Concat, Dots, Len,
    Eof,
}

struct Lexer<'a> {
    input: std::iter::Peekable<std::str::Chars<'a>>,
}

impl<'a> Lexer<'a> {
    fn new(input: &'a str) -> Self {
        let mut chars = input.chars().peekable();
        // Skip shebang
        if chars.peek() == Some(&'#') {
            for c in chars.by_ref() {
                if c == '\n' { break; }
            }
        }
        Self {
            input: chars,
        }
    }

    fn next_token(&mut self) -> Result<Token, LuaError> {
        self.skip_whitespace_and_comments();
        let c = match self.input.next() {
            Some(c) => c,
            None => return Ok(Token::Eof),
        };

        match c {
            '+' => Ok(Token::Plus),
            '*' => Ok(Token::Mul),
            '%' => Ok(Token::Mod),
            '^' => Ok(Token::Pow),
            '&' => Ok(Token::BAnd),
            '|' => Ok(Token::BOr),
            '#' => Ok(Token::Len),
            '(' => Ok(Token::LParen),
            ')' => Ok(Token::RParen),
            '{' => Ok(Token::LCurly),
            '}' => Ok(Token::RCurly),
            '[' => {
                let mut level = 0;
                while self.input.peek() == Some(&'=') {
                    level += 1;
                    self.input.next();
                }
                if self.input.peek() == Some(&'[') {
                    self.input.next();
                    let s = self.read_long_bracket(level)?;
                    Ok(Token::String(s))
                } else if level == 0 {
                    Ok(Token::LBracket)
                } else {
                    Err(LuaError::SyntaxError("invalid long bracket".to_string()))
                }
            }
            ']' => Ok(Token::RBracket),
            ';' => Ok(Token::Semi),
            ',' => Ok(Token::Comma),
            '-' => {
                if self.input.peek() == Some(&'-') {
                    self.input.next();
                    self.skip_comment()?;
                    self.next_token()
                } else {
                    Ok(Token::Minus)
                }
            }
            '/' => {
                if self.input.peek() == Some(&'/') {
                    self.input.next();
                    Ok(Token::IDiv)
                } else {
                    Ok(Token::Div)
                }
            }
            '~' => {
                if self.input.peek() == Some(&'=') {
                    self.input.next();
                    Ok(Token::Ne)
                } else {
                    Ok(Token::BXor)
                }
            }
            '=' => {
                if self.input.peek() == Some(&'=') {
                    self.input.next();
                    Ok(Token::Eq)
                } else {
                    Ok(Token::Assign)
                }
            }
            '<' => {
                match self.input.peek() {
                    Some(&'=') => { self.input.next(); Ok(Token::Le) }
                    Some(&'<') => { self.input.next(); Ok(Token::Shl) }
                    _ => Ok(Token::Lt)
                }
            }
            '>' => {
                match self.input.peek() {
                    Some(&'=') => { self.input.next(); Ok(Token::Ge) }
                    Some(&'>') => { self.input.next(); Ok(Token::Shr) }
                    _ => Ok(Token::Gt)
                }
            }
            '.' => {
                if self.input.peek() == Some(&'.') {
                    self.input.next();
                    if self.input.peek() == Some(&'.') {
                        self.input.next();
                        Ok(Token::Dots)
                    } else {
                        Ok(Token::Concat)
                    }
                } else {
                    Ok(Token::Dot)
                }
            }
            ':' => Ok(Token::Colon),
            '"' | '\'' => self.read_string(c),
            c if c.is_ascii_digit() => self.read_number(c),
            c if c.is_alphabetic() || c == '_' => self.read_name(c),
            _ => Err(LuaError::SyntaxError(format!("unexpected character: {}", c))),
        }
    }

    fn skip_whitespace_and_comments(&mut self) {
        loop {
            match self.input.peek() {
                Some(&c) if c.is_whitespace() => { self.input.next(); }
                _ => break,
            }
        }
    }

    fn skip_comment(&mut self) -> Result<(), LuaError> {
        if self.input.peek() == Some(&'[') {
            self.input.next();
            let mut level = 0;
            while self.input.peek() == Some(&'=') {
                level += 1;
                self.input.next();
            }
            if self.input.peek() == Some(&'[') {
                self.input.next();
                self.read_long_bracket(level)?;
                return Ok(());
            }
        }
        for c in self.input.by_ref() {
            if c == '\n' { break; }
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
        while let Some(c) = self.input.next() {
            if c == quote {
                return Ok(Token::String(s));
            }
            if c == '\\' {
                let next = self.input.next().ok_or(LuaError::SyntaxError("unfinished string".to_string()))?;
                match next {
                    'a' => s.push(0x07),
                    'b' => s.push(0x08),
                    'f' => s.push(0x0C),
                    'n' => s.push(b'\n'),
                    'r' => s.push(b'\r'),
                    't' => s.push(b'\t'),
                    'v' => s.push(0x0B),
                    '\\' => s.push(b'\\'),
                    '"' => s.push(b'"'),
                    '\'' => s.push(b'\''),
                    '\n' => s.push(b'\n'),
                    'z' => {
                        while let Some(&c) = self.input.peek() {
                            if c.is_whitespace() {
                                self.input.next();
                            } else {
                                break;
                            }
                        }
                    }
                    'x' => {
                        let h1 = self.input.next().ok_or(LuaError::SyntaxError("unfinished hex escape".to_string()))?;
                        let h2 = self.input.next().ok_or(LuaError::SyntaxError("unfinished hex escape".to_string()))?;
                        let hex = format!("{}{}", h1, h2);
                        let val = u8::from_str_radix(&hex, 16).map_err(|_| LuaError::SyntaxError("invalid hex escape".to_string()))?;
                        s.push(val);
                    }
                    'u' => {
                        if self.input.next() != Some('{') { return Err(LuaError::SyntaxError("expected '{' in unicode escape".to_string())); }
                        let mut hex = String::new();
                        loop {
                            let c = self.input.next().ok_or(LuaError::SyntaxError("unfinished unicode escape".to_string()))?;
                            if c == '}' { break; }
                            hex.push(c);
                        }
                        let val = u32::from_str_radix(&hex, 16).map_err(|_| LuaError::SyntaxError("invalid unicode escape".to_string()))?;
                        Self::encode_utf8_extended(val, &mut s);
                    }
                    c if c.is_ascii_digit() => {
                        let mut dec = c.to_string();
                        for _ in 0..2 {
                            if let Some(&nc) = self.input.peek() {
                                if nc.is_ascii_digit() {
                                    dec.push(self.input.next().unwrap());
                                } else {
                                    break;
                                }
                            }
                        }
                        let val = dec.parse::<u16>().map_err(|_| LuaError::SyntaxError("invalid decimal escape".to_string()))?;
                        if val > 255 { return Err(LuaError::SyntaxError("decimal escape too large".to_string())); }
                        s.push(val as u8);
                    }
                    _ => return Err(LuaError::SyntaxError(format!("invalid escape sequence: \\{}", next))),
                }
            } else {
                let mut buf = [0u8; 4];
                let bytes = c.encode_utf8(&mut buf).as_bytes();
                s.extend_from_slice(bytes);
            }
        }
        Err(LuaError::SyntaxError("unfinished string".to_string()))
    }

    fn read_long_bracket(&mut self, level: usize) -> Result<Vec<u8>, LuaError> {
        if self.input.peek() == Some(&'\n') {
            self.input.next();
        }
        let mut s = Vec::new();
        loop {
            match self.input.next() {
                Some(']') => {
                    let mut count = 0;
                    while self.input.peek() == Some(&'=') {
                        count += 1;
                        self.input.next();
                    }
                    if count == level && self.input.peek() == Some(&']') {
                        self.input.next();
                        return Ok(s);
                    } else {
                        s.push(b']');
                        s.extend(std::iter::repeat_n(b'=', count));
                    }
                }
                Some(c) => {
                    let mut buf = [0u8; 4];
                    let bytes = c.encode_utf8(&mut buf).as_bytes();
                    s.extend_from_slice(bytes);
                }
                None => return Err(LuaError::SyntaxError("unfinished long string/comment".to_string())),
            }
        }
    }

    fn read_number(&mut self, first: char) -> Result<Token, LuaError> {
        let mut s = first.to_string();
        let mut is_float = false;
        while let Some(&c) = self.input.peek() {
            if c.is_ascii_digit() {
                s.push(self.input.next().unwrap());
            } else if c == '.' && !is_float {
                is_float = true;
                s.push(self.input.next().unwrap());
            } else {
                break;
            }
        }
        if is_float {
            Ok(Token::Number(s.parse().map_err(|_| LuaError::SyntaxError("invalid number".to_string()))?))
        } else {
            Ok(Token::Integer(s.parse().map_err(|_| LuaError::SyntaxError("invalid integer".to_string()))?))
        }
    }

    fn read_name(&mut self, first: char) -> Result<Token, LuaError> {
        let mut s = first.to_string();
        while let Some(&c) = self.input.peek() {
            if c.is_alphanumeric() || c == '_' {
                s.push(self.input.next().unwrap());
            } else {
                break;
            }
        }
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
            vec![UpvalDesc { name: "_ENV".to_string(), instack: true, idx: 0 }]
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
            maxstacksize: 2, // minimum
            declared_globals: HashMap::new(),
            global_const_all: false,
        }
    }

    fn add_k(&mut self, val: Value) -> usize {
        for (i, v) in self.k.iter().enumerate() {
            if v == &val { return i; }
        }
        self.k.push(val);
        self.k.len() - 1
    }

    fn resolve_local(&self, name: &str) -> Option<usize> {
        self.locals.iter().rev().find(|l| l.name == name).map(|l| l.reg)
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
    states: Vec<CompileState>,
    heap: &'a mut GcHeap,
}

impl<'a> Parser<'a> {
    pub fn new(input: &'a str, heap: &'a mut GcHeap) -> Result<Self, LuaError> {
        let mut lexer = Lexer::new(input);
        let lookahead = lexer.next_token()?;
        Ok(Self {
            lexer,
            lookahead,
            states: vec![CompileState::new(true)],
            heap,
        })
    }

    fn consume(&mut self) -> Result<Token, LuaError> {
        let old = self.lookahead.clone();
        self.lookahead = self.lexer.next_token()?;
        Ok(old)
    }

    fn peek(&self) -> &Token {
        &self.lookahead
    }

    fn expect(&mut self, expected: Token) -> Result<(), LuaError> {
        let token = self.consume()?;
        if token == expected {
            Ok(())
        } else {
            Err(LuaError::SyntaxError(format!("expected {:?}, got {:?}", expected, token)))
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
        self.emit(OpCode::Return0 as u32); // RETURN0
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
                    // Emit CLOSE reg
                    let reg = local.reg;
                    state.instructions.push(Instruction(OpCode::Close as u32 | ((reg as u32) << 7)));
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
            Token::If => {
                self.parse_if_statement()?;
            }
            Token::While => {
                self.parse_while_statement()?;
            }
            Token::Repeat => {
                self.parse_repeat_statement()?;
            }
            Token::For => {
                self.parse_for_statement()?;
            }
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
                // Placeholder for BREAK
                return Err(LuaError::SyntaxError("break not yet fully supported".to_string()));
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
                    self.parse_global_function()?;
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
            Token::LParen => {
                self.parse_call_statement_with_paren()?;
            }
            _ => {
                return Err(LuaError::SyntaxError(format!("unexpected token in statement: {:?}", self.peek())));
            }
        }
        Ok(())
    }

    fn parse_name_statement(&mut self, name: String) -> Result<(), LuaError> {
        if self.peek() == &Token::Assign {
            self.consume()?;
            self.parse_assignment(name)?;
            return Ok(());
        }

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
                    self.parse_call(reg, 0)?;
                    self.current_state().pop_regs(1);
                    return Ok(());
                }
                _ => break,
            }
        }

        // If we get here, it might have been just a name which is not a valid statement
        self.current_state().pop_regs(1);
        Ok(())
    }

    fn parse_call_statement_with_paren(&mut self) -> Result<(), LuaError> {
        let reg = self.current_state().push_reg();
        self.parse_primary(reg)?;
        // After primary, it MUST have been a call to be a valid statement.
        // For now, we don't strictly enforce it.
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
        while !matches!(self.peek(), Token::End | Token::Else | Token::Elseif | Token::Eof) {
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
        self.current_state().instructions[jmp_to_next] = Instruction(OpCode::Jmp as u32 | (((diff + 0xFFFFFF) as u32) << 7));

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
            while !matches!(self.peek(), Token::End | Token::Else | Token::Elseif | Token::Eof) {
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
            self.current_state().instructions[jmp_to_next] = Instruction(OpCode::Jmp as u32 | (((diff + 0xFFFFFF) as u32) << 7));
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
            self.current_state().instructions[jmp_idx] = Instruction(OpCode::Jmp as u32 | (((diff + 0xFFFFFF) as u32) << 7));
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
        self.current_state().instructions[jmp_to_end] = Instruction(OpCode::Jmp as u32 | (((diff_end + 0xFFFFFF) as u32) << 7));

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

        // Loop until condition is true.
        // TEST A k=0 -> skip next if (not R[A] == 0) i.e. if R[A] is true.
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
                return Err(LuaError::SyntaxError("expected name after comma".to_string()));
            }
        }

        if self.peek() == &Token::Assign {
            if names.len() != 1 { return Err(LuaError::SyntaxError("numeric for must have exactly one variable".to_string())); }
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
            // FORPREP A sBx
            self.emit(OpCode::ForPrep as u32 | ((base_reg as u32) << 7));

            self.enter_scope();
            let depth = self.current_state().scope_depth;
            self.current_state().locals.push(Local { name, depth, reg: base_reg + 3, is_const: false, is_close: false });
            self.current_state().next_reg += 1; // for local variable

            while self.peek() != &Token::End && self.peek() != &Token::Eof {
                self.parse_statement()?;
            }
            self.exit_scope();
            self.expect(Token::End)?;

            let loop_idx = self.current_state().instructions.len();
            let diff = loop_idx as i32 - prep_idx as i32 - 1;
            // FORPREP Sj (Wait, my VM uses Sj for jumps but FORPREP uses sBx usually)
            // Let's check my VM loop. It uses Sj for JMP.
            // If I use sBx, it's bits 15-31.
            self.current_state().instructions[prep_idx] = Instruction(OpCode::ForPrep as u32 | ((base_reg as u32) << 7) | (((diff + 0xFFFF) as u32) << 15));
            // FORLOOP A sBx (jumps back to start of body)
            let back_diff = prep_idx as i32 - loop_idx as i32 - 1;
            self.emit(OpCode::ForLoop as u32 | ((base_reg as u32) << 7) | (((back_diff + 0xFFFF) as u32) << 15));

            self.current_state().pop_regs(4);
            Ok(())
        } else if self.peek() == &Token::In {
            self.consume()?; // in
            Err(LuaError::SyntaxError("generic for not yet supported".to_string()))
        } else {
            Err(LuaError::SyntaxError("expected '=' or 'in' in for loop".to_string()))
        }
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
                    return Err(LuaError::SyntaxError("expected name in global declaration".to_string()));
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

    fn parse_global_function(&mut self) -> Result<(), LuaError> {
        // global function name body
        self.parse_function_definition(false)
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
                    return Err(LuaError::SyntaxError("expected name after comma in local declaration".to_string()));
                }
            }

            if self.peek() == &Token::Assign {
                self.consume()?;
                let start_reg = self.current_state().next_reg;
                for i in 0..names.len() {
                    let reg = self.current_state().push_reg();
                    if i == names.len() - 1 {
                        self.parse_expression(reg)?;
                    } else {
                        self.parse_expression(reg)?;
                        if self.peek() == &Token::Comma { self.consume()?; }
                    }
                }
                for (i, (name, attr)) in names.into_iter().zip(attributes.into_iter()).enumerate() {
                    let is_const = attr == "const";
                    let is_close = attr == "close";
                    let reg = start_reg + i;
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
                    self.emit(OpCode::LoadNil as u32 | ((reg as u32) << 7)); // LOADNIL
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
            Err(LuaError::SyntaxError("expected name in local declaration".to_string()))
        }
    }

    fn parse_assignment(&mut self, name: String) -> Result<(), LuaError> {
        self.check_const_assignment(&name)?;
        let dest_reg = self.current_state().push_reg();
        self.parse_expression(dest_reg)?;
        self.emit_store(name, dest_reg)?;
        self.current_state().pop_regs(1);
        Ok(())
    }

    fn emit_store(&mut self, name: String, src_reg: usize) -> Result<(), LuaError> {
        if let Some(reg) = self.current_state().resolve_local(&name) {
            self.emit(OpCode::Move as u32 | ((reg as u32) << 7) | ((src_reg as u32) << 24));
        } else if let Some(uv_idx) = self.resolve_upvalue(&name) {
            self.emit(OpCode::SetUpval as u32 | ((src_reg as u32) << 7) | ((uv_idx as u32) << 15));
        } else {
            let k_name = self.add_string_k(name);
            self.emit(
                OpCode::SetTabUp as u32 | ((k_name as u32) << 24) | ((src_reg as u32) << 16) | (1 << 15),
            );
        }
        Ok(())
    }

    fn resolve_upvalue(&mut self, name: &str) -> Option<usize> {
        if self.states.len() <= 1 { return None; }

        for (i, uv) in self.states.last().unwrap().upvalues.iter().enumerate() {
            if uv.name == name { return Some(i); }
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

            if depth == 0 { break; }
            depth -= 1;
        }
        None
    }

    fn parse_call(&mut self, func_reg: usize, nresults: i32) -> Result<(), LuaError> {
        self.parse_call_internal(func_reg, nresults, false)
    }

    fn parse_call_internal(&mut self, func_reg: usize, nresults: i32, has_self: bool) -> Result<(), LuaError> {
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
                    return Err(LuaError::SyntaxError("colon call with string literal not supported".to_string()));
                }
                let arg_reg = self.current_state().push_reg();
                let s_gc = self.heap.allocate(s);
                let k = self.current_state().add_k(Value::String(s_gc));
                self.emit(OpCode::LoadK as u32 | ((arg_reg as u32) << 7) | ((k as u32) << 15));
                self.current_state().pop_regs(1);
                2
            }
            Token::LCurly => {
                return Err(LuaError::SyntaxError("table constructor as function argument not yet supported".to_string()));
            }
            _ => return Err(LuaError::SyntaxError(format!("expected function arguments, got {:?}", self.peek()))),
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
            let k_name = self.add_string_k(name);
            self.emit(
                OpCode::GetTabUp as u32
                    | ((dest_reg as u32) << 7)
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
        if !matches!(self.peek(), Token::End | Token::Eof | Token::Else | Token::Elseif | Token::Until) {
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
        if !is_local {
            if let Token::Name(name) = self.consume()? {
                name_parts.push(name);
                while self.peek() == &Token::Dot {
                    self.consume()?;
                    if let Token::Name(name) = self.consume()? {
                        name_parts.push(name);
                    }
                }
            }
        } else if let Token::Name(name) = self.consume()? {
                name_parts.push(name);
        }

        self.states.push(CompileState::new(false));
        self.expect(Token::LParen)?;
        let mut numparams = 0;
        let mut is_vararg = false;
        if self.peek() != &Token::RParen {
            loop {
                if self.peek() == &Token::Dots {
                    self.consume()?;
                    is_vararg = true;
                    break;
                }
                if let Token::Name(arg_name) = self.consume()? {
                    let reg = self.current_state().push_reg();
                    self.current_state().locals.push(Local { name: arg_name, depth: 0, reg, is_const: false, is_close: false });
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
            if prec <= min_prec { break; }

            self.consume()?;

            if token == Token::And || token == Token::Or {
                self.parse_logical_op(token, dest_reg, prec)?;
            } else {
                let right_reg = self.current_state().push_reg();
                let next_min_prec = if prec == 12 || prec == 8 { prec - 1 } else { prec };
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
        self.emit(OpCode::TestSet as u32 | ((dest_reg as u32) << 7) | ((dest_reg as u32) << 24) | (k << 15));
        let jmp_idx = self.current_state().instructions.len();
        self.emit(OpCode::Jmp as u32);
        self.parse_binop(dest_reg, prec)?;
        let end_idx = self.current_state().instructions.len();
        let diff = end_idx as i32 - jmp_idx as i32 - 1;
        self.current_state().instructions[jmp_idx] = Instruction(OpCode::Jmp as u32 | (((diff + 0xFFFFFF) as u32) << 7));
        Ok(())
    }

    fn parse_unary(&mut self, dest_reg: usize) -> Result<(), LuaError> {
        let token = self.peek().clone();
        if token == Token::Not || token == Token::Len || token == Token::Minus || token == Token::BXor {
            self.consume()?;
            self.parse_unary(dest_reg)?;
            match token {
                Token::Minus => self.emit(OpCode::Unm as u32 | ((dest_reg as u32) << 7) | ((dest_reg as u32) << 24)),
                Token::Not => self.emit(OpCode::Not as u32 | ((dest_reg as u32) << 7) | ((dest_reg as u32) << 24)),
                Token::Len => self.emit(OpCode::Len as u32 | ((dest_reg as u32) << 7) | ((dest_reg as u32) << 24)),
                Token::BXor => self.emit(OpCode::BNot as u32 | ((dest_reg as u32) << 7) | ((dest_reg as u32) << 24)),
                _ => unreachable!(),
            }
        } else {
            self.parse_primary(dest_reg)?;
        }
        Ok(())
    }

    fn parse_primary(&mut self, dest_reg: usize) -> Result<(), LuaError> {
        match self.consume()? {
            Token::Integer(i) => {
                if (-32768..=32767).contains(&i) {
                    let val = (i + 0xFFFF) as u32;
                    self.emit(OpCode::LoadI as u32 | ((dest_reg as u32) << 7) | (val << 15));
                } else {
                    let k = self.current_state().add_k(Value::Integer(i));
                    self.emit(OpCode::LoadK as u32 | ((dest_reg as u32) << 7) | ((k as u32) << 15));
                }
            }
            Token::Number(n) => {
                let k = self.current_state().add_k(Value::Number(n));
                self.emit(OpCode::LoadK as u32 | ((dest_reg as u32) << 7) | ((k as u32) << 15));
            }
            Token::String(s) => {
                let s_gc = self.heap.allocate(s);
                let k = self.current_state().add_k(Value::String(s_gc));
                self.emit(OpCode::LoadK as u32 | ((dest_reg as u32) << 7) | ((k as u32) << 15));
            }
            Token::True => {
                self.emit(OpCode::LoadTrue as u32 | ((dest_reg as u32) << 7) | (1 << 24));
            }
            Token::False => {
                self.emit(OpCode::LoadFalse as u32 | ((dest_reg as u32) << 7));
            }
            Token::Nil => {
                self.emit(OpCode::LoadNil as u32 | ((dest_reg as u32) << 7));
            }
            Token::Name(name) => {
                self.emit_load(name, dest_reg)?;
                self.parse_primary_suffix(dest_reg)?;
            }
            Token::Dots => {
                self.emit(OpCode::VarArg as u32 | ((dest_reg as u32) << 7) | (2 << 24));
            }
            Token::Function => {
                self.parse_function_definition(false)?;
                let last_reg = self.current_state().next_reg - 1;
                if last_reg != dest_reg {
                    self.emit(OpCode::Move as u32 | ((dest_reg as u32) << 7) | ((last_reg as u32) << 24));
                    self.current_state().pop_regs(1);
                }
            }
            Token::LParen => {
                self.parse_expression(dest_reg)?;
                self.expect(Token::RParen)?;
                self.parse_primary_suffix(dest_reg)?;
            }
            _ => return Err(LuaError::SyntaxError("expected expression".to_string())),
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

    fn emit_binop(&mut self, op: Token, dest: usize, left: usize, right: usize) -> Result<(), LuaError> {
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
            _ => return Err(LuaError::SyntaxError(format!("operator {:?} not yet fully supported in expressions", op))),
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
