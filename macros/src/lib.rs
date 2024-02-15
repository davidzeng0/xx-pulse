use proc_macro::TokenStream;
use quote::ToTokens;
use syn::*;

#[proc_macro_attribute]
pub fn main(_: TokenStream, item: TokenStream) -> TokenStream {
	let mut func = match parse::<ItemFn>(item) {
		Ok(func) => func,
		Err(err) => return err.to_compile_error().into()
	};

	let mut main = func.clone();
	let ident = &func.sig.ident;

	func.attrs.clear();
	main.sig.asyncness.take();
	main.block = parse_quote! {{
		#[::xx_pulse::asynchronous]
		#func

		::xx_pulse::Runtime::new()
			.unwrap()
			.block_on(#ident())
	}};

	main.to_token_stream().into()
}
