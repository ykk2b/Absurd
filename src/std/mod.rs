use std::{cell::RefCell, rc::Rc};
pub mod core;

use crate::{
    ast::{DeclrFuncType, FuncValType, LiteralType},
    interpreter::env::{Env, FuncKind},
};

pub fn func(name: &str, arity: usize, env: &mut Rc<RefCell<Env>>, func: Rc<dyn FuncValType>) {
    let params = vec![];
    env.borrow().define_pub_func(
        name.to_string(),
        LiteralType::DeclrFunc(DeclrFuncType {
            name: name.to_string(),
            arity,
            func,
        }),
        FuncKind {
            params,
            is_async: false,
            is_pub: false,
            is_impl: false,
            is_mut: false,
        },
    )
}
