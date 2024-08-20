use crate::{
    ast::{
        FuncBody, FuncImpl, FuncValueType, LiteralType,
        Statement::{self, *},
        Token,
    },
    env::Env,
    errors::{Error, ErrorCode::*},
    expr::Expression,
    resolver::type_check,
    std::core::io::StdCoreIo,
};
use std::{collections::HashMap, process::exit};

#[derive(Debug)]
pub struct Interpreter {
    pub env: Env,
    pub specs: HashMap<String, LiteralType>,
    pub is_mod: bool,
    error: Error,
}

impl Interpreter {
    pub fn new(src: &str) -> Self {
        let int = Self {
            env: Env::new(HashMap::new()),
            specs: HashMap::new(),
            is_mod: false,
            error: Error::new(src),
        };
        let mut std_core_io = StdCoreIo::new(int.env.clone());
        std_core_io.load();
        int
    }
    pub fn new_with_env(env: Env, src: &str) -> Self {
        Self {
            env,
            specs: HashMap::new(),
            is_mod: false,
            error: Error::new(src),
        }
    }
    pub fn interpret(&mut self, stmts: Vec<&Statement>) {
        for stmt in stmts {
            match stmt {
                Statement::Expression { expr } => {
                    expr.eval(self.env.clone());
                }
                Block { stmts } => {
                    if !self.is_mod {
                        let new_env = self.env.enclose();
                        let prev_env = self.env.clone();
                        self.env = new_env;
                        self.interpret(stmts.iter().map(|x| x).collect());
                        self.env = prev_env;
                    }
                }
                Var {
                    names,
                    value,
                    is_pub,
                    pub_names,
                    is_func,
                    ..
                } => match value {
                    Some(v) => {
                        if !self.is_mod {
                            if is_func.clone() {
                                if names.len() != 1 {
                                    self.error
                                        .throw(E0x401, names[0].line, names[0].pos, vec![]);
                                }
                                let call = self.create_func(stmt);
                                let func = LiteralType::Func(FuncValueType::Func(call));
                                self.env.define(names[0].lexeme.clone(), func);
                            } else {
                                let val = v.eval(self.env.clone());
                                for name in names {
                                    self.env.define(name.lexeme.clone(), val.clone());
                                }
                                if is_pub.clone() {
                                    for name in pub_names {
                                        self.env.define_pub(name.lexeme.clone(), val.clone());
                                    }
                                }
                            }
                        } else if is_pub.clone() {
                            let val = v.eval(self.env.clone());
                            for name in pub_names {
                                self.env.define_pub(name.lexeme.clone(), val.clone());
                            }
                        }
                    }
                    None => {
                        if is_pub.clone() {
                            self.error
                                .throw(E0x402, names[0].line, names[0].pos, vec![]);
                        }
                        let val = LiteralType::Null;
                        for name in names {
                            self.env.define(name.lexeme.clone(), val.clone());
                        }
                    }
                },
                Func { name, is_pub, .. } => {
                    // @todo handle implementation,
                    // asynchroneity and param mutability

                    let call = self.create_func(stmt);
                    let func = LiteralType::Func(FuncValueType::Func(call));
                    if is_pub.clone() {
                        self.env.define_pub(name.lexeme.clone(), func.clone());
                    } else if !self.is_mod {
                        self.env.define(name.lexeme.clone(), func);
                    }
                }
                If {
                    cond,
                    body,
                    else_branch,
                    else_if_branches,
                } => {
                    if !self.is_mod {
                        let val = cond.eval(self.env.clone());
                        if val.is_truthy() {
                            self.interpret(body.iter().map(|x| x).collect());
                        } else {
                            let mut executed = false;
                            for (cond, body) in else_if_branches {
                                let val = cond.eval(self.env.clone());
                                if val.is_truthy() {
                                    executed = true;
                                    self.interpret(body.iter().map(|x| x).collect());
                                    break;
                                }
                            }
                            if let Some(body) = else_branch {
                                if !executed {
                                    self.interpret(body.iter().map(|x| x).collect());
                                }
                            }
                        }
                    }
                }
                Return { expr } => {
                    let value = expr.eval(self.env.clone());
                    self.specs.insert("return".to_string(), value);
                }
                While { cond, body } => {
                    if !self.is_mod {
                        while cond.eval(self.env.clone()).is_truthy() {
                            self.interpret(body.iter().map(|x| x).collect());
                            if self.specs.get("break").is_some() {
                                self.specs.remove("break");
                                break;
                            }
                        }
                    }
                }
                Loop { iter, body } => {
                    if !self.is_mod {
                        match iter {
                            Some(i) => {
                                for _ in 0..i.clone() {
                                    self.interpret(body.iter().map(|x| x).collect());
                                    if self.specs.get("break").is_some() {
                                        self.specs.remove("break");
                                        break;
                                    }
                                }
                            }
                            None => loop {
                                self.interpret(body.iter().map(|x| x).collect());
                                if self.specs.get("break").is_some() {
                                    self.specs.remove("break");
                                    break;
                                }
                            },
                        }
                    }
                }
                Break {} => {
                    self.specs.insert("break".to_string(), LiteralType::Null);
                }
                Match { .. } => {
                    // @todo handle match statements
                }
                Mod { .. } => {
                    // @todo handle mod statements
                }
                Use { .. } => {
                    // @todo handle use statements
                }
                Struct { .. } => {
                    // @todo handle struct statements
                }
                Impl { .. } => {
                    // @todo handle impl statements
                }
                Enum { .. } => {
                    // @todo handle enum statements
                }
            }
        }
    }
    fn create_func(&self, stmt: &Statement) -> FuncImpl {
        if let Func {
            name,
            value_type,
            body,
            params,
            is_async,
            is_pub,
            is_impl,
            is_mut,
        } = stmt
        {
            let params: Vec<(Token, Token)> = params
                .iter()
                .map(|(name, value_type)| (name.clone(), value_type.clone()))
                .collect();
            let body: Vec<Statement> = match body {
                FuncBody::Statements(stmts) => stmts.iter().map(|x| x.clone()).collect(),
                _ => {
                    self.error.throw(E0x403, name.line, name.pos, vec![]);
                    exit(1);
                }
            };

            FuncImpl {
                name: name.lexeme.clone(),
                value_type: value_type.clone(),
                body: FuncBody::Statements(body),
                params,
                is_async: *is_async,
                is_pub: *is_pub,
                is_impl: *is_impl,
                is_mut: *is_mut,
                env: Env::new(HashMap::new()),
            }
        } else {
            self.error.throw(E0x404, 0, (0, 0), vec![]);
            exit(1);
        }
    }
}

pub fn run_func(func: FuncImpl, args: &Vec<Expression>, env: Env) -> LiteralType {
    let error = Error::new("");
    if args.len() != func.params.len() {
        error.throw(E0x405, 0, (0, 0), vec![]);
    }
    let mut arg_values = vec![];
    for arg in args {
        arg_values.push(arg.eval(env.clone()));
    }
    let func_env = func.env.enclose();
    for (i, val) in arg_values.iter().enumerate() {
        if i < func.params.len() {
            if !type_check(&func.value_type, &val) {
                error.throw(
                    E0x301,
                    0,
                    (0, 0),
                    vec![val.to_string(), arg_values[i].to_string()],
                );
            }
            func_env.define(func.params[i].0.lexeme.clone(), val.clone());
        } else {
            error.throw(E0x405, 0, (0, 0), vec![]);
        }
    }
    let mut int = Interpreter::new_with_env(func_env, "");

    match func.body {
        FuncBody::Statements(body) => {
            for stmt in body {
                int.interpret(vec![&stmt]);
                let val = int.specs.get("return");

                if val.is_some() {
                    let v = val.unwrap().clone();
                    if !type_check(&func.value_type, &v) {
                        error.throw(
                            E0x301,
                            0,
                            (0, 0),
                            vec![func.value_type.clone().lexeme, v.to_string()],
                        );
                    }
                    return v;
                }
            }
        }
        _ => {
            error.throw(E0x403, 0, (0, 0), vec![]);
        }
    }

    if func.value_type.lexeme != "void" {
        error.throw(E0x406, 0, (0, 0), vec![]);
    }
    LiteralType::Null
}
