use crate::error::LuaError;
use crate::vm::{Proto, Instruction, UpvalDesc, OpCode};
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
    Concat, Dots, Len,
    Eof,
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

    fn skip_comment(&mut self) {
        // Skip second '-'
        for c in self.input.by_ref() {
            if c == '\n' { break; }
        }
    }

    fn read_string(&mut self, quote: char) -> Result<Token, LuaError> {
        let mut s = String::new();
        for c in self.input.by_ref() {
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
    protos: Vec<crate::gc::Gc<Proto>>,
    scope_depth: usize,
    next_reg: usize,
    k: Vec<Value>,
    instructions: Vec<Instruction>,
    numparams: u8,
    is_vararg: bool,
    maxstacksize: u8,
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
                state.locals.pop();
                state.next_reg -= 1;
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
                while self.peek() != &Token::End && self.peek() != &Token::Eof {
                    self.parse_statement()?;
                }
                self.expect(Token::End)?;
                self.exit_scope();
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
            Token::Function => {
                self.consume()?;
                self.parse_function_definition(false)?;
            }
            Token::Return => {
                self.consume()?;
                self.parse_return_statement()?;
            }
            Token::Name(name) => {
                self.consume()?;
                if self.peek() == &Token::Assign {
                    self.consume()?;
                    self.parse_assignment(name)?;
                } else if self.peek() == &Token::LParen {
                    self.parse_call_statement(name)?;
                } else if self.peek() == &Token::Dot {
                    // Handle table assignment: t.f = ...
                    self.parse_table_assignment(name)?;
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
        let mut names = Vec::new();
        if let Token::Name(name) = self.consume()? {
            names.push(name);
            while self.peek() == &Token::Comma {
                self.consume()?;
                if let Token::Name(name) = self.consume()? {
                    names.push(name);
                } else {
                    return Err(LuaError::SyntaxError("expected name after comma in local declaration".to_string()));
                }
            }

            if self.peek() == &Token::Assign {
                self.consume()?;
                let start_reg = self.current_state().next_reg;
                // Simplified: parse each expression into a new register
                for i in 0..names.len() {
                    let reg = self.current_state().push_reg();
                    if i == names.len() - 1 {
                        // For the last one, it might be a multi-return call
                        self.parse_expression(reg)?;
                    } else {
                        self.parse_expression(reg)?;
                        if self.peek() == &Token::Comma { self.consume()?; }
                    }
                }
                for (i, name) in names.into_iter().enumerate() {
                    let state = self.current_state();
                    state.locals.push(Local {
                        name,
                        depth: state.scope_depth,
                        reg: start_reg + i,
                    });
                }
            } else {
                for name in names {
                    let reg = self.current_state().push_reg();
                    self.emit(OpCode::LoadNil as u32 | ((reg as u32) << 7)); // LOADNIL
                    let state = self.current_state();
                    state.locals.push(Local {
                        name,
                        depth: state.scope_depth,
                        reg,
                    });
                }
            }
            Ok(())
        } else {
            Err(LuaError::SyntaxError("expected name in local declaration".to_string()))
        }
    }

    fn parse_assignment(&mut self, name: String) -> Result<(), LuaError> {
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
            // SETUPVAL src_reg uv_idx
            self.emit(OpCode::SetUpval as u32 | ((src_reg as u32) << 7) | ((uv_idx as u32) << 15));
        } else {
            // Global (SETTABUP _ENV)
            let s_gc = self.heap.allocate(name);
            let k_name = self.current_state().add_k(Value::String(s_gc));
            // Op=14 (SETTABUP), A=0 (_ENV), B=k_name, C=src_reg, k=1
            self.emit(OpCode::SetTabUp as u32 | ((k_name as u32) << 24) | ((src_reg as u32) << 16) | (1 << 15));
        }
        Ok(())
    }

    fn resolve_upvalue(&mut self, name: &str) -> Option<usize> {
        if self.states.len() <= 1 { return None; }

        // Check if already in current state's upvalues
        for (i, uv) in self.states.last().unwrap().upvalues.iter().enumerate() {
            if uv.name == name { return Some(i); }
        }

        // Try to resolve in outer states
        let mut depth = self.states.len() - 2;
        loop {
            // Check locals in this outer state
            if let Some(reg) = self.states[depth].resolve_local(name) {
                // Found! Now add to all states from depth+1 to end
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

            // Check upvalues in this outer state
            for (i, uv) in self.states[depth].upvalues.iter().enumerate() {
                if uv.name == name {
                    // Found!
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

    fn parse_call_statement(&mut self, name: String) -> Result<(), LuaError> {
        let func_reg = self.current_state().push_reg();
        self.emit_load(name, func_reg)?;
        self.parse_call(func_reg, 0)?;
        self.current_state().pop_regs(1);
        Ok(())
    }

    fn parse_call(&mut self, func_reg: usize, nresults: i32) -> Result<(), LuaError> {
        self.expect(Token::LParen)?;
        let mut arg_count = 0;
        let mut vararg_call = false;
        if self.peek() != &Token::RParen {
            loop {
                let arg_reg = self.current_state().push_reg();
                if self.peek() == &Token::Dots {
                    self.consume()?;
                    // VARARG arg_reg 0
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

        // CALL R[func_reg] B=arg_count+1 (or 0 if vararg) C=nresults+1
        let b = if vararg_call { 0 } else { arg_count + 1 };
        let c = (nresults + 1) as u32;
        self.emit(OpCode::Call as u32 | ((func_reg as u32) << 7) | ((b as u32) << 24) | (c << 15));

        self.current_state().pop_regs(arg_count);
        Ok(())
    }

    fn emit_load(&mut self, name: String, dest_reg: usize) -> Result<(), LuaError> {
        if let Some(reg) = self.current_state().resolve_local(&name) {
            self.emit(OpCode::Move as u32 | ((dest_reg as u32) << 7) | ((reg as u32) << 24));
        } else if let Some(uv_idx) = self.resolve_upvalue(&name) {
            // GETUPVAL dest_reg uv_idx
            self.emit(OpCode::GetUpval as u32 | ((dest_reg as u32) << 7) | ((uv_idx as u32) << 15));
        } else {
            // Global (GETTABUP _ENV)
            let s_gc = self.heap.allocate(name);
            let k_name = self.current_state().add_k(Value::String(s_gc));
            self.emit(OpCode::GetTabUp as u32 | ((dest_reg as u32) << 7) | ((k_name as u32) << 16) | (1 << 15));
        }
        Ok(())
    }

    fn parse_expression(&mut self, dest_reg: usize) -> Result<(), LuaError> {
        self.parse_binop(dest_reg, 0)
    }

    fn parse_table_assignment(&mut self, table_name: String) -> Result<(), LuaError> {
        let table_reg = self.current_state().push_reg();
        self.emit_load(table_name, table_reg)?;
        self.expect(Token::Dot)?;
        if let Token::Name(field_name) = self.consume()? {
            self.expect(Token::Assign)?;
            let val_reg = self.current_state().push_reg();
            self.parse_expression(val_reg)?;

            let s_gc = self.heap.allocate(field_name);
            let k_field = self.current_state().add_k(Value::String(s_gc));
            // SETFIELD A B C (SETFIELD R[A] K[B] R[C])
            // Op=17, A=table_reg, B=k_field, C=val_reg
            self.emit(OpCode::SetField as u32 | ((table_reg as u32) << 7) | ((k_field as u32) << 24) | ((val_reg as u32) << 15));

            self.current_state().pop_regs(2);
        }
        Ok(())
    }

    fn parse_return_statement(&mut self) -> Result<(), LuaError> {
        let mut nres = 0;
        let start_reg = self.current_state().next_reg;
        if self.peek() != &Token::End && self.peek() != &Token::Eof && self.peek() != &Token::Else && self.peek() != &Token::Elseif && self.peek() != &Token::Until {
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
        // RETURN A B
        // A = start_reg, B = nres + 1
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
                    self.current_state().locals.push(Local { name: arg_name, depth: 0, reg });
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
            self.emit(OpCode::VarArgPrep as u32 | ((numparams as u32) << 7)); // VARARGPREP
        }

        while self.peek() != &Token::End && self.peek() != &Token::Eof {
            self.parse_statement()?;
        }
        self.expect(Token::End)?;
        self.emit(OpCode::Return0 as u32); // RETURN0

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
            });
        } else if name_parts.len() == 1 {
            self.emit_store(name_parts[0].clone(), dest_reg)?;
            self.current_state().pop_regs(1);
        } else if name_parts.len() > 1 {
            // table field: t.f.g = func
            // This is simplified: only support t.f
            let table_reg = self.current_state().push_reg();
            self.emit_load(name_parts[0].clone(), table_reg)?;
            let s_gc = self.heap.allocate(name_parts[1].clone());
            let k_field = self.current_state().add_k(Value::String(s_gc));
            self.emit(OpCode::SetField as u32 | ((table_reg as u32) << 7) | ((k_field as u32) << 24) | ((dest_reg as u32) << 15));
            self.current_state().pop_regs(2);
        } else {
             // Anonymous - leave it in dest_reg
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
            let right_reg = self.current_state().push_reg();

            let next_min_prec = if prec == 12 || prec == 8 { prec - 1 } else { prec };
            self.parse_binop(right_reg, next_min_prec)?;

            self.emit_binop(op, dest_reg, dest_reg, right_reg)?;
            self.current_state().pop_regs(1);
        }
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
                Token::BXor => self.emit(OpCode::BNot as u32 | ((dest_reg as u32) << 7) | ((dest_reg as u32) << 24)), // BNOT
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
                    let val = (i + 0xFFFF) as u32; // Simplified signed handling
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
                while self.peek() == &Token::LParen {
                    self.parse_call(dest_reg, 1)?;
                }
            }
            Token::Dots => {
                // VARARG dest_reg 2 (load 1 vararg)
                self.emit(OpCode::VarArg as u32 | ((dest_reg as u32) << 7) | (2 << 24));
            }
            Token::Function => {
                // Anonymous function
                self.parse_function_definition(false)?;
                // Result is in the register allocated by parse_function_definition.
                // But parse_function_definition allocates its own register.
                // This is a bit messy, let's fix it.
                // For now, assume it works if we adjust it.
                // Actually, parse_function_definition puts it in a new register.
                // We want it in dest_reg.
                let last_reg = self.current_state().next_reg - 1;
                if last_reg != dest_reg {
                    self.emit(((dest_reg as u32) << 7) | ((last_reg as u32) << 24));
                    self.current_state().pop_regs(1);
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
            // EQ A=l B=r k=1
            self.emit(opcode | (l << 7) | (r << 24) | (1 << 15));
        } else if op == Token::Gt || op == Token::Ge {
             // Swap operands
             self.emit(opcode | (r << 7) | (l << 24));
        } else if op == Token::Eq || op == Token::Lt || op == Token::Le {
             self.emit(opcode | (l << 7) | (r << 24));
        } else {
            self.emit(opcode | (d << 7) | (l << 24) | (r << 16)); // Arithmetic/bitwise
        }
        Ok(())
    }
}
