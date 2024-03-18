use proc_macro::TokenStream;
use quote::ToTokens;
use syn::{visit_mut::VisitMut, *};
use xx_macro_support::macro_expr::visit_macro_punctuated_exprs;

struct HasAwait(bool);

impl VisitMut for HasAwait {
	fn visit_item_mut(&mut self, _: &mut Item) {}

	fn visit_expr_closure_mut(&mut self, _: &mut ExprClosure) {}

	fn visit_expr_await_mut(&mut self, _: &mut ExprAwait) {
		self.0 = true;
	}

	fn visit_macro_mut(&mut self, mac: &mut Macro) {
		visit_macro_punctuated_exprs(self, mac);
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
		let (sig, ident, block) = (&func.sig, &func.sig.ident, &func.block);

		func.attrs.clear();
		main.block = parse_quote! {{
			#[::xx_pulse::asynchronous]
			#sig {
				#(#sync)*

				::xx_pulse::Runtime::new()
					.unwrap()
					.block_on(async move #block)
			}

			::xx_core::coroutines::Task::run(#ident(), ::xx_core::pointer::Ptr::null())
		}};
	}

	main.to_token_stream().into()
}
