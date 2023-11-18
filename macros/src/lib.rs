use proc_macro::TokenStream;
use quote::quote;
use syn::*;

#[proc_macro_attribute]
pub fn main(
	_: TokenStream, item: TokenStream
) -> TokenStream {
	let mut func = match parse::<ItemFn>(item) {
		Ok(func) => {
			func
		}

		Err(err) => {
			return err.to_compile_error().into();
		}
	};

	let return_type = func.sig.output.clone();
	let ident = func.sig.ident.clone();
	let attrs = func.attrs;

	func.attrs = Vec::new();

	quote! {
		#(#attrs)*
		fn #ident () #return_type {
			#[xx_pulse::async_fn]
			#func

			xx_pulse::Runtime::new()
				.unwrap()
				.block_on(#ident())
		}
	}.into()
}
