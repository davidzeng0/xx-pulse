#![allow(clippy::wildcard_imports)]

use proc_macro::TokenStream;
use quote::ToTokens;
use syn::visit_mut::VisitMut;
use syn::*;
use xx_macro_support::attribute::*;
use xx_macro_support::visit_macro::*;

struct HasAwait(bool);

impl VisitMut for HasAwait {
	fn visit_item_mut(&mut self, _: &mut Item) {}

	fn visit_expr_async_mut(&mut self, _: &mut ExprAsync) {}

	fn visit_expr_closure_mut(&mut self, _: &mut ExprClosure) {}

	fn visit_expr_await_mut(&mut self, _: &mut ExprAwait) {
		self.0 = true;
	}

	fn visit_macro_mut(&mut self, mac: &mut Macro) {
		visit_macro_body(self, mac);
	}
}

#[proc_macro_attribute]
pub fn main(attr: TokenStream, item: TokenStream) -> TokenStream {
	let mut func = match ensure_empty(attr.into()).and_then(|()| parse::<ItemFn>(item)) {
		Ok(func) => func,
		Err(err) => return err.to_compile_error().into()
	};

	func.sig.asyncness.take();
	func.attrs.push(parse_quote! {
		#[::xx_pulse::asynchronous(sync)]
	});

	let pos = func.block.stmts.iter_mut().position(|stmt| {
		let mut has_await = HasAwait(false);

		has_await.visit_stmt_mut(stmt);
		has_await.0
	});

	if let Some(pos) = pos {
		let sync: Vec<_> = func.block.stmts.drain(0..pos).collect();
		let block = &func.block;

		func.block = parse_quote! {{
			#(#sync)*

			::xx_pulse::Runtime::new()
				.expect("Failed to start runtime")
				.block_on(async #block)
		}};
	}

	func.to_token_stream().into()
}
