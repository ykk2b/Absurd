// Absurd resolver, it resolves statements and returns locals
use crate::ast::{FuncBody, Statement, Token};
use crate::errors::{Error, ErrorCode::*};
use crate::interpreter::env::Env;
use crate::interpreter::expr::Expression;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Debug, Clone)]
pub struct Resolver {
    locals: HashMap<usize, usize>,
    scopes: Vec<HashMap<String, bool>>,
    is_crnt_fnc: bool,
    is_crnt_loop: bool,
    err: Error,
}

impl Resolver {
    pub fn new(err: Error) -> Self {
        Resolver {
            locals: HashMap::new(),
            scopes: Vec::new(),
            is_crnt_fnc: false,
            is_crnt_loop: false,
            err,
        }
    }

    /// entry method
    pub fn resolve(
        &mut self,
        stmts: &[Statement],
        env: &Rc<RefCell<Env>>,
    ) -> HashMap<usize, usize> {
        stmts.iter().for_each(|stmt| self.resolve_stmt(stmt, env));
        self.locals.clone()
    }

    /// statement resolver
    fn resolve_stmt(&mut self, stmt: &Statement, env: &Rc<RefCell<Env>>) {
        match stmt {
            Statement::If { .. } => self.ifs(stmt, env),
            Statement::Block { .. } => self.block(stmt, env),
            Statement::Break {} => self.breaks(),
            Statement::Enum { .. } => self.enums(stmt),
            Statement::Expression { expr } => self.expr(expr, env),
            Statement::Func { .. } => self.func(stmt, env),
            Statement::Loop { .. } => self.loops(stmt, env),
            Statement::Match { .. } => self.matchs(stmt, env),
            Statement::Return { .. } => self.returns(stmt, env),
            Statement::Use { .. } => self.uses(stmt),
            Statement::Var { .. } => self.var(stmt, env),
            Statement::While { .. } => self.whiles(stmt, env),
            _ => {}
        }
    }

    // everything below is self explanatory
    fn uses(&mut self, stmt: &Statement) {
        if let Statement::Use { names, .. } = stmt {
            for (old, new) in names {
                if let Some(new_name) = new {
                    self.declare(new_name);
                    self.define(new_name);
                } else {
                    self.declare(old);
                    self.define(old);
                }
            }
        }
    }

    fn var(&mut self, stmt: &Statement, env: &Rc<RefCell<Env>>) {
        if let Statement::Var { names, value, .. } = stmt {
            for name in names {
                self.declare(name);
                if let Some(value) = value {
                    self.expr(value, env);
                }
                self.define(name);
            }
        }
    }

    fn whiles(&mut self, stmt: &Statement, env: &Rc<RefCell<Env>>) {
        if let Statement::While { body, cond } = stmt {
            let encl_loop = self.is_crnt_loop;
            self.expr(cond, env);
            self.is_crnt_loop = true;
            self.resolve_many(body, env);
            self.is_crnt_loop = encl_loop;
        }
    }

    fn breaks(&mut self) {
        if !self.is_crnt_loop {
            self.err.throw(E0x302, 0, (0, 0), vec![]);
        }
    }

    fn enums(&mut self, stmt: &Statement) {
        if let Statement::Enum { name, .. } = stmt {
            self.declare(name);
            self.define(name);
        }
    }

    fn func(&mut self, stmt: &Statement, env: &Rc<RefCell<Env>>) {
        if let Statement::Func { body, params, .. } = stmt {
            let encl_func = self.is_crnt_fnc;
            self.is_crnt_fnc = true;
            self.scope_start();
            for (parname, _) in params {
                self.declare(parname);
                self.define(parname);
            }

            if let FuncBody::Statements(body) = body {
                self.resolve_many(body, env);
                for stmt in body {
                    if let Statement::Return { expr } = stmt {
                        self.expr(expr, env);
                    }
                }
            } else {
                self.err.throw(E0x305, 0, (0, 0), vec![]);
            }

            self.scope_end();
            self.is_crnt_fnc = encl_func;
        }
    }

    fn loops(&mut self, stmt: &Statement, env: &Rc<RefCell<Env>>) {
        if let Statement::Loop { body, .. } = stmt {
            self.scope_start();
            let encl_loop = self.is_crnt_loop;
            self.is_crnt_loop = true;
            self.resolve_many(body, env);
            self.is_crnt_loop = encl_loop;
            self.scope_end();
        }
    }

    fn matchs(&mut self, stmt: &Statement, env: &Rc<RefCell<Env>>) {
        if let Statement::Match {
            cond,
            def_case,
            cases,
        } = stmt
        {
            self.expr(cond, env);
            for (case, body) in cases {
                self.scope_start();
                self.expr(case, env);
                match body {
                    FuncBody::Statements(stmts) => {
                        self.resolve_many(&stmts, env);
                    }
                    FuncBody::Expression(expr) => {
                        self.expr(expr, env);
                    }
                }
                self.scope_end();
            }

            match def_case {
                FuncBody::Statements(stmts) => {
                    if !stmts.is_empty() {
                        self.scope_start();
                        self.resolve_many(stmts, env);
                        self.scope_end();
                    }
                }
                FuncBody::Expression(expr) => {
                    self.expr(expr, env);
                }
            }
        }
    }

    fn returns(&mut self, stmt: &Statement, env: &Rc<RefCell<Env>>) {
        if self.is_crnt_fnc {
            if let Statement::Return { expr } = stmt {
                self.expr(expr, env);
            }
        } else {
            self.err.throw(E0x303, 0, (0, 0), vec![]);
        }
    }

    fn ifs(&mut self, stmt: &Statement, env: &Rc<RefCell<Env>>) {
        if let Statement::If {
            cond,
            body,
            else_if_branches,
            else_branch,
        } = stmt
        {
            self.expr(cond, env);
            self.scope_start();
            self.resolve_many(body, env);
            self.scope_end();
            for (elif_pred, elif_stmt) in else_if_branches {
                self.expr(elif_pred, env);
                self.scope_start();
                self.resolve_many(elif_stmt, env);
                self.scope_end();
            }
            if let Some(branch) = else_branch {
                self.scope_start();
                self.resolve_many(branch, env);
                self.scope_end();
            }
        }
    }

    fn block(&mut self, stmt: &Statement, env: &Rc<RefCell<Env>>) {
        if let Statement::Block { stmts } = stmt {
            self.scope_start();
            self.resolve_many(stmts, env);
            self.scope_end();
        } else {
            self.err
                .throw(E0x306, 0, (0, 0), vec!["a block statement".to_string()]);
        }
    }

    fn expr(&mut self, expr: &Expression, env: &Rc<RefCell<Env>>) {
        match expr {
            Expression::Object { fields, .. } => {
                fields.iter().for_each(|(_, val)| {
                    self.expr(val, env);
                });
            }
            Expression::Method { args, .. } => args.iter().for_each(|arg| self.expr(arg, env)),
            Expression::Assign { value, .. } => self.expr(value, env),
            Expression::Array { items, .. } => {
                items.iter().for_each(|item| self.expr(item, env));
            }
            Expression::Var { .. } => self.varexpr(expr),
            Expression::Call { name, args, .. } => {
                self.expr(name.as_ref(), env);
                args.iter().for_each(|arg| self.expr(arg, env));
            }
            Expression::Func { body, params, .. } => self.callback(body, params, env),
            Expression::Await { expr, .. } => self.expr(expr, env),
            Expression::Unary { left, .. } => self.expr(left, env),
            Expression::Value { .. } => {}
            Expression::Binary { left, right, .. } => {
                self.expr(left, env);
                self.expr(right, env);
            }
            Expression::Grouping { expression, .. } => self.expr(expression, env),
        }
    }

    fn callback(&mut self, body: &FuncBody, params: &[(Token, Token)], env: &Rc<RefCell<Env>>) {
        let encl_func = self.is_crnt_fnc;
        self.is_crnt_fnc = true;
        self.scope_start();
        for (parname, _) in params {
            self.declare(parname);
            self.define(parname);
        }
        match body {
            FuncBody::Statements(body) => {
                self.resolve_many(body, env);
                body.iter().for_each(|stmt| {
                    if let Statement::Return { expr } = stmt {
                        self.expr(expr, env);
                    }
                });
            }
            FuncBody::Expression(expr) => {
                self.expr(expr, env);
                self.expr(expr, env);
            }
        }

        self.scope_end();
        self.is_crnt_fnc = encl_func;
    }

    fn varexpr(&mut self, expr: &Expression) {
        if let Expression::Var { name, .. } = expr {
            if let Some(false) = self.scopes.last().and_then(|scope| scope.get(&name.lexeme)) {
                self.err.throw(
                    E0x306,
                    name.line,
                    name.pos,
                    vec!["a local variable".to_string()],
                );
            }
        } else if let Expression::Call { name, .. } = expr {
            if let Expression::Var { name, .. } = name.as_ref() {
                self.resolve_local(name, expr.id());
            } else {
                self.err
                    .throw(E0x306, 0, (0, 0), vec!["a variable".to_string()]);
            }
        } else {
            self.err
                .throw(E0x306, 0, (0, 0), vec!["a variable".to_string()]);
        }
    }

    fn declare(&mut self, name: &Token) {
        if let Some(scope) = self.scopes.last_mut() {
            if scope.contains_key(&name.lexeme) {
                self.err
                    .throw(E0x307, name.line, name.pos, vec![name.lexeme.clone()]);
            }
            scope.insert(name.lexeme.clone(), false);
        }
    }

    fn define(&mut self, name: &Token) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.lexeme.clone(), true);
        }
    }

    fn resolve_local(&mut self, name: &Token, id: usize) {
        for (i, scope) in self.scopes.iter().rev().enumerate() {
            if scope.contains_key(&name.lexeme) {
                self.locals.insert(id, i);
                return;
            }
        }
    }

    fn resolve_many(&mut self, stmts: &[Statement], env: &Rc<RefCell<Env>>) {
        stmts.iter().for_each(|stmt| self.resolve_stmt(stmt, env));
    }

    fn scope_start(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn scope_end(&mut self) {
        if self.scopes.pop().is_none() {
            self.err.throw(E0x308, 0, (0, 0), vec![]);
        }
    }
}
