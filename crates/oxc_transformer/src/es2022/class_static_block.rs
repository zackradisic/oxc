use oxc_ast::{ast::*, AstBuilder};
use oxc_span::{Atom, Span};

use std::{collections::HashSet, rc::Rc};

/// ES2022: Class Static Block
///
/// References:
/// * <https://babel.dev/docs/babel-plugin-transform-class-static-block>
/// * <https://github.com/babel/babel/blob/main/packages/babel-plugin-transform-class-static-block>
pub struct ClassStaticBlock<'a> {
    ast: Rc<AstBuilder<'a>>,
}

impl<'a> ClassStaticBlock<'a> {
    pub fn new(ast: Rc<AstBuilder<'a>>) -> Self {
        Self { ast }
    }

    pub fn transform_class_body<'b>(&mut self, class_body: &'b mut ClassBody<'a>) {
        if !class_body.body.iter().any(|e| matches!(e, ClassElement::StaticBlock(..))) {
            return;
        }

        let private_names: HashSet<Atom> = class_body
            .body
            .iter()
            .filter_map(ClassElement::property_key)
            .filter_map(PropertyKey::private_name)
            .collect();

        let mut i = 0;
        for element in class_body.body.iter_mut() {
            let ClassElement::StaticBlock(block) = element else {
                continue;
            };

            let span = block.span;

            let static_block_private_id = generate_uid(&private_names, &mut i);
            let key = PropertyKey::PrivateIdentifier(self.ast.alloc(PrivateIdentifier {
                span: Span::default(),
                name: static_block_private_id.clone(),
            }));

            let value = match block.body.len() {
                0 => None,
                1 if matches!(block.body[0], Statement::ExpressionStatement(..)) => {
                    // We special-case the single expression case to avoid the iife, since it's common.
                    //
                    // We prefer to emit:
                    // ```JavaScript
                    // class Foo {
                    //   static bar = 42;
                    //   static #_ = this.foo = Foo.bar;
                    // }
                    // ```
                    // instead of:
                    // ```JavaScript
                    // class Foo {
                    //   static bar = 42;
                    //   static #_ = (() => this.foo = Foo.bar)();
                    // }
                    // ```

                    let stmt = self.ast.move_statement(&mut (*block.body)[0]);
                    let Statement::ExpressionStatement(mut expr_stmt) = stmt else {
                        unreachable!()
                    };
                    let value = self.ast.move_expression(&mut expr_stmt.expression);
                    Some(value)
                }
                _ => {
                    let params = self.ast.formal_parameters(
                        Span::default(),
                        FormalParameterKind::ArrowFormalParameters,
                        self.ast.new_vec(),
                        None,
                    );

                    let statements = self.ast.move_statement_vec(&mut block.body);
                    let function_body =
                        self.ast.function_body(Span::default(), self.ast.new_vec(), statements);

                    let callee = self.ast.arrow_expression(
                        Span::default(),
                        false,
                        false,
                        false,
                        params,
                        function_body,
                        None,
                        None,
                    );

                    let callee = self.ast.parenthesized_expression(Span::default(), callee);

                    let value = self.ast.call_expression(
                        Span::default(),
                        callee,
                        self.ast.new_vec(),
                        false,
                        None,
                    );
                    Some(value)
                }
            };

            *element = self.ast.class_property(span, key, value, false, true, self.ast.new_vec());
        }
    }
}

fn generate_uid(deny_list: &HashSet<Atom>, i: &mut u32) -> Atom {
    *i += 1;

    let mut uid: Atom = if *i == 1 { "_".to_string() } else { format!("_{i}") }.into();
    while deny_list.contains(&uid) {
        *i += 1;
        uid = format!("_{i}").into();
    }

    uid
}
