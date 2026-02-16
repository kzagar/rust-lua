use crate::error::LuaError;
use crate::vm::{Proto, Instruction, UpvalDesc};
use crate::value::Value;
use crate::gc::GcHeap;

#[derive(Debug, PartialEq, Clone)]
enum Token {
    Name(String),
    Number(f64),
    Integer(i64),
    String(String),
    // Keywords
    Local, Nil, True, False, And, Or, Not, If, Then, Else, Elseif, End,
    While, Do, Repeat, Until, For, In, Function, Return, Break,
    // Operators
    Plus, Minus, Mul, Div, IDiv, Mod, Pow,
    BAnd, BOr, BXor, Shl, Shr,
    Eq, Ne, Lt, Gt, Le, Ge,
    Assign, Dot, Comma, Semi, Colon,
    LParen, RParen, LCurly, RCurly, LBracket, RBracket,
    Concat, Len,
    EOF,
}

struct Lexer<'a> {
    input: std::iter::Peekable<std::str::Chars<'a>>,
}

impl<'a> Lexer<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input: input.chars().peekable(),
        }
    }

    fn next_token(&mut self) -> Result<Token, LuaError> {
        self.skip_whitespace_and_comments();
        let c = match self.input.next() {
            Some(c) => c,
            None => return Ok(Token::EOF),
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
            '[' => Ok(Token::LBracket),
            ']' => Ok(Token::RBracket),
            ';' => Ok(Token::Semi),
            ',' => Ok(Token::Comma),
            '-' => {
                if self.input.peek() == Some(&'-') {
                    self.skip_comment();
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
                    Ok(Token::Concat)
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

    fn skip_comment(&mut self) {
        // Skip second '-'
        while let Some(c) = self.input.next() {
            if c == '\n' { break; }
        }
    }

    fn read_string(&mut self, quote: char) -> Result<Token, LuaError> {
        let mut s = String::new();
        while let Some(c) = self.input.next() {
            if c == quote {
                return Ok(Token::String(s));
            }
            s.push(c);
        }
        Err(LuaError::SyntaxError("unfinished string".to_string()))
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
            _ => Ok(Token::Name(s)),
        }
    }
}

struct Local {
    name: String,
    depth: usize,
    reg: usize,
}

struct CompileState {
    locals: Vec<Local>,
    upvalues: Vec<UpvalDesc>,
    scope_depth: usize,
    next_reg: usize,
    k: Vec<Value>,
    instructions: Vec<Instruction>,
}

impl CompileState {
    fn new() -> Self {
        Self {
            locals: Vec::new(),
            upvalues: vec![UpvalDesc { name: "_ENV".to_string(), instack: true, idx: 0 }],
            scope_depth: 0,
            next_reg: 0,
            k: Vec::new(),
            instructions: Vec::new(),
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
        for local in self.locals.iter().rev() {
            if local.name == name {
                return Some(local.reg);
            }
        }
        None
    }

    fn push_reg(&mut self) -> usize {
        let r = self.next_reg;
        self.next_reg += 1;
        r
    }

    fn pop_regs(&mut self, n: usize) {
        self.next_reg -= n;
    }
}

pub struct Parser<'a> {
    lexer: Lexer<'a>,
    lookahead: Token,
    state: CompileState,
    heap: &'a mut GcHeap,
}

impl<'a> Parser<'a> {
    pub fn new(input: &'a str, heap: &'a mut GcHeap) -> Result<Self, LuaError> {
        let mut lexer = Lexer::new(input);
        let lookahead = lexer.next_token()?;
        Ok(Self {
            lexer,
            lookahead,
            state: CompileState::new(),
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

    fn emit(&mut self, instr: u32) {
        self.state.instructions.push(Instruction(instr));
    }

    pub fn parse_chunk(mut self) -> Result<Proto, LuaError> {
        while self.peek() != &Token::EOF {
            self.parse_statement()?;
        }
        // Add implicit return
        self.emit(70 | (0 << 7)); // RETURN0
        Ok(Proto {
            instructions: self.state.instructions,
            k: self.state.k,
            upvalues: self.state.upvalues,
            protos: vec![],
        })
    }

    fn enter_scope(&mut self) {
        self.state.scope_depth += 1;
    }

    fn exit_scope(&mut self) {
        self.state.scope_depth -= 1;
        while let Some(local) = self.state.locals.last() {
            if local.depth > self.state.scope_depth {
                self.state.locals.pop();
                self.state.next_reg -= 1;
            } else {
                break;
            }
        }
    }

    fn parse_statement(&mut self) -> Result<(), LuaError> {
        match self.peek().clone() {
            Token::Do => {
                self.consume()?;
                self.enter_scope();
                while self.peek() != &Token::End && self.peek() != &Token::EOF {
                    self.parse_statement()?;
                }
                self.expect(Token::End)?;
                self.exit_scope();
            }
            Token::Local => {
                self.consume()?;
                self.parse_local_declaration()?;
            }
            Token::Name(name) => {
                self.consume()?;
                if self.peek() == &Token::Assign {
                    self.consume()?;
                    self.parse_assignment(name)?;
                } else if self.peek() == &Token::LParen {
                    self.parse_call_statement(name)?;
                } else {
                    return Err(LuaError::SyntaxError("unexpected token after name".to_string()));
                }
            }
            _ => {
                return Err(LuaError::SyntaxError(format!("unexpected token in statement: {:?}", self.peek())));
            }
        }
        Ok(())
    }

    fn parse_local_declaration(&mut self) -> Result<(), LuaError> {
        if let Token::Name(name) = self.consume()? {
            self.expect(Token::Assign)?;
            let reg = self.state.push_reg();
            self.parse_expression(reg)?;
            self.state.locals.push(Local {
                name,
                depth: self.state.scope_depth,
                reg,
            });
            Ok(())
        } else {
            Err(LuaError::SyntaxError("expected name in local declaration".to_string()))
        }
    }

    fn parse_assignment(&mut self, name: String) -> Result<(), LuaError> {
        let dest_reg = self.state.push_reg();
        self.parse_expression(dest_reg)?;
        if let Some(reg) = self.state.resolve_local(&name) {
            // MOVE R[reg] R[dest_reg]
            self.emit(0 | ((reg as u32) << 7) | ((dest_reg as u32) << 24));
        } else {
            let s_gc = self.heap.allocate(name);
            let k_name = self.state.add_k(Value::String(s_gc));
            // SETTABUP _ENV key=B val=C
            // Op=14 (SETTABUP), A=0 (_ENV), B=k_name, C=dest_reg, k=1
            self.emit(14 | (0 << 7) | ((k_name as u32) << 24) | ((dest_reg as u32) << 16) | (1 << 15));
        }
        self.state.pop_regs(1);
        Ok(())
    }

    fn parse_call_statement(&mut self, name: String) -> Result<(), LuaError> {
        let func_reg = self.state.push_reg();
        if let Some(reg) = self.state.resolve_local(&name) {
            self.emit(0 | ((func_reg as u32) << 7) | ((reg as u32) << 24));
        } else {
            let s_gc = self.heap.allocate(name);
            let k_name = self.state.add_k(Value::String(s_gc));
            // GETTABUP R[func_reg] _ENV K[k_name]
            self.emit(10 | ((func_reg as u32) << 7) | (0 << 24) | ((k_name as u32) << 16) | (1 << 15));
        }

        self.expect(Token::LParen)?;
        let mut arg_count = 0;
        if self.peek() != &Token::RParen {
            loop {
                let arg_reg = self.state.push_reg();
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

        // CALL R[func_reg] B=arg_count+1 C=1
        self.emit(67 | ((func_reg as u32) << 7) | (((arg_count + 1) as u32) << 24) | (1 << 15));

        self.state.pop_regs(arg_count + 1);
        Ok(())
    }

    fn parse_expression(&mut self, dest_reg: usize) -> Result<(), LuaError> {
        self.parse_binop(dest_reg, 0)
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
            Token::Not | Token::Len => 11, // Unary actually handled separately but for loop logic...
            Token::Pow => 12,
            _ => 0,
        }
    }

    fn parse_binop(&mut self, dest_reg: usize, min_prec: i32) -> Result<(), LuaError> {
        self.parse_unary(dest_reg)?;

        loop {
            let prec = Self::get_precedence(self.peek());
            if prec <= min_prec { break; }

            let op = self.consume()?;
            let right_reg = self.state.push_reg();

            let next_min_prec = if prec == 12 || prec == 8 { prec - 1 } else { prec };
            self.parse_binop(right_reg, next_min_prec)?;

            self.emit_binop(op, dest_reg, dest_reg, right_reg)?;
            self.state.pop_regs(1);
        }
        Ok(())
    }

    fn parse_unary(&mut self, dest_reg: usize) -> Result<(), LuaError> {
        let token = self.peek().clone();
        if token == Token::Not || token == Token::Len || token == Token::Minus || token == Token::BXor {
            self.consume()?;
            self.parse_unary(dest_reg)?;
            match token {
                Token::Minus => self.emit(48 | ((dest_reg as u32) << 7) | ((dest_reg as u32) << 24)),
                Token::Not => self.emit(50 | ((dest_reg as u32) << 7) | ((dest_reg as u32) << 24)),
                Token::Len => self.emit(51 | ((dest_reg as u32) << 7) | ((dest_reg as u32) << 24)),
                Token::BXor => self.emit(49 | ((dest_reg as u32) << 7) | ((dest_reg as u32) << 24)), // BNOT
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
                if i >= -32768 && i <= 32767 {
                    let val = (i + 0xFFFF) as u32; // Simplified signed handling
                    self.emit(2 | ((dest_reg as u32) << 7) | (val << 15));
                } else {
                    let k = self.state.add_k(Value::Integer(i));
                    self.emit(1 | ((dest_reg as u32) << 7) | ((k as u32) << 15));
                }
            }
            Token::Number(n) => {
                let k = self.state.add_k(Value::Number(n));
                self.emit(1 | ((dest_reg as u32) << 7) | ((k as u32) << 15));
            }
            Token::String(s) => {
                let s_gc = self.heap.allocate(s);
                let k = self.state.add_k(Value::String(s_gc));
                self.emit(1 | ((dest_reg as u32) << 7) | ((k as u32) << 15));
            }
            Token::True => {
                self.emit(6 | ((dest_reg as u32) << 7) | (1 << 24));
            }
            Token::False => {
                self.emit(4 | ((dest_reg as u32) << 7));
            }
            Token::Nil => {
                self.emit(7 | ((dest_reg as u32) << 7) | (0 << 24));
            }
            Token::Name(name) => {
                if let Some(reg) = self.state.resolve_local(&name) {
                    self.emit(0 | ((dest_reg as u32) << 7) | ((reg as u32) << 24));
                } else {
                    let s_gc = self.heap.allocate(name);
                    let k_name = self.state.add_k(Value::String(s_gc));
            self.emit(10 | ((dest_reg as u32) << 7) | (0 << 24) | ((k_name as u32) << 16) | (1 << 15));
                }
            }
            Token::LParen => {
                self.parse_expression(dest_reg)?;
                self.expect(Token::RParen)?;
            }
            _ => return Err(LuaError::SyntaxError("expected expression".to_string())),
        }
        Ok(())
    }

    fn emit_binop(&mut self, op: Token, dest: usize, left: usize, right: usize) -> Result<(), LuaError> {
        let opcode = match op {
            Token::Plus => 33,
            Token::Minus => 34,
            Token::Mul => 35,
            Token::Mod => 36,
            Token::Pow => 37,
            Token::Div => 38,
            Token::IDiv => 39,
            Token::BAnd => 40,
            Token::BOr => 41,
            Token::BXor => 42,
            Token::Shl => 43,
            Token::Shr => 44,
            Token::Concat => 52,
            Token::Eq => 56,
            Token::Ne => 56,
            Token::Lt => 57,
            Token::Gt => 57,
            Token::Le => 58,
            Token::Ge => 58,
            _ => return Err(LuaError::SyntaxError(format!("operator {:?} not yet fully supported in expressions", op))),
        };
        let d = dest as u32;
        let l = left as u32;
        let r = right as u32;
        if op == Token::Ne {
            // EQ A=l B=r k=1
            self.emit(opcode | (l << 7) | (r << 24) | (1 << 15));
        } else if op == Token::Gt || op == Token::Ge {
             // Swap operands
             self.emit(opcode | (r << 7) | (l << 24));
        } else if op == Token::Eq || op == Token::Lt || op == Token::Le {
             self.emit(opcode | (l << 7) | (r << 24));
        } else {
            self.emit(opcode | (d << 7) | (l << 24) | (r << 15)); // Arithmetic/bitwise
        }
        Ok(())
    }
}
