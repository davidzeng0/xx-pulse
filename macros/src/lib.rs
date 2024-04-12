use proc_macro::TokenStream;
use quote::ToTokens;
use syn::{visit_mut::VisitMut, *};
use xx_macro_support::visit_macro::*;

struct HasAwait(bool);

impl VisitMut for HasAwait {
	fn visit_item_mut(&mut self, _: &mut Item) {}

	fn visit_expr_closure_mut(&mut self, _: &mut ExprClosure) {}

	fn visit_expr_await_mut(&mut self, _: &mut ExprAwait) {
		self.0 = true;
	}

	fn visit_macro_mut(&mut self, mac: &mut Macro) {
		visit_macro_body(self, mac);
	}
}

#[proc_macro_attribute]
pub fn main(_: TokenStream, item: TokenStream) -> TokenStream {
	let mut func = match parse::<ItemFn>(item) {
		Ok(func) => func,
		Err(err) => return err.to_compile_error().into()
	};

	let mut main = func.clone();

	main.sig.asyncness.take();

	let pos = func.block.stmts.iter_mut().position(|stmt| {
		let mut has_await = HasAwait(false);

		has_await.visit_stmt_mut(stmt);
		has_await.0
	});

	if let Some(pos) = pos {
		let sync: Vec<_> = func.block.stmts.drain(0..pos).collect();
		let (ident, block) = (&func.sig.ident, &func.block);

		func.attrs.clear();
		main.block = parse_quote! {{
			#(#sync)*

			::xx_pulse::Runtime::new()
				.unwrap()
				.block_on({
					#[::xx_pulse::asynchronous(block)]
					async fn #ident() #block
				})
		}};
	}

	main.to_token_stream().into()
}
