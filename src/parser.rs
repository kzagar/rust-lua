use crate::error::LuaError;
use crate::vm::{Proto, Instruction};
use crate::value::Value;

#[derive(Debug, PartialEq, Clone)]
enum Token {
    Name(String),
    Number(f64),
    Integer(i64),
    Plus,
    Minus,
    Equal,
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
        self.skip_whitespace();
        match self.input.next() {
            Some('+') => Ok(Token::Plus),
            Some('-') => Ok(Token::Minus),
            Some('=') => Ok(Token::Equal),
            Some(c) if c.is_ascii_digit() => {
                let mut s = c.to_string();
                while let Some(&next) = self.input.peek() {
                    if next.is_ascii_digit() {
                        s.push(self.input.next().unwrap());
                    } else {
                        break;
                    }
                }
                Ok(Token::Integer(s.parse().unwrap()))
            }
            Some(c) if c.is_alphabetic() => {
                let mut s = c.to_string();
                while let Some(&next) = self.input.peek() {
                    if next.is_alphanumeric() {
                        s.push(self.input.next().unwrap());
                    } else {
                        break;
                    }
                }
                Ok(Token::Name(s))
            }
            None => Ok(Token::EOF),
            _ => Err(LuaError::SyntaxError("unexpected character".to_string())),
        }
    }

    fn skip_whitespace(&mut self) {
        while let Some(&c) = self.input.peek() {
            if c.is_whitespace() {
                self.input.next();
            } else {
                break;
            }
        }
    }
}

pub struct Parser<'a> {
    lexer: Lexer<'a>,
    lookahead: Token,
}

impl<'a> Parser<'a> {
    pub fn new(input: &'a str) -> Result<Self, LuaError> {
        let mut lexer = Lexer::new(input);
        let lookahead = lexer.next_token()?;
        Ok(Self { lexer, lookahead })
    }

    fn consume(&mut self) -> Result<Token, LuaError> {
        let old = self.lookahead.clone();
        self.lookahead = self.lexer.next_token()?;
        Ok(old)
    }

    pub fn parse_expr(mut self) -> Result<Proto, LuaError> {
        let mut instructions = Vec::new();
        let mut k = Vec::new();

        // Extremely simplified: handles 'Integer + Integer'
        if let Token::Integer(i1) = self.consume()? {
            k.push(Value::Integer(i1));
            instructions.push(Instruction(1 | (0 << 7) | (0 << 15))); // LOADK R[0] K[0]

            if let Token::Plus = self.consume()? {
                if let Token::Integer(i2) = self.consume()? {
                    k.push(Value::Integer(i2));
                    instructions.push(Instruction(1 | (1 << 7) | (1 << 15))); // LOADK R[1] K[1]
                    instructions.push(Instruction(2 | (2 << 7) | (0 << 16) | (1 << 24))); // ADD R[2] R[0] R[1]
                }
            }
        }

        Ok(Proto { instructions, k })
    }
}
