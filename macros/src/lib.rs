use std::mem::take;

use proc_macro::TokenStream;
use quote::quote;
use syn::*;

#[proc_macro_attribute]
pub fn main(_: TokenStream, item: TokenStream) -> TokenStream {
	let mut func = match parse::<ItemFn>(item) {
		Ok(func) => func,
		Err(err) => return err.to_compile_error().into()
	};

	let ident = &func.sig.ident;
	let return_type = &func.sig.output;
	let attrs = take(&mut func.attrs);

	quote! {
		#(#attrs)*
		fn #ident () #return_type {
			#[::xx_pulse::asynchronous]
			#func

			::xx_pulse::Runtime::new()
				.unwrap()
				.block_on(#ident())
		}
	}
	.into()
}
