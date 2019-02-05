use proc_macro::TokenStream;

use crate::measure_opts::MeasureRequestAttribute;
use crate::metered_opts::MeteredKeyValAttribute;

use aspect_weave::*;
use std::rc::Rc;
use synattra::ParseAttributes;

pub fn metered(attrs: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    let woven_impl_block = weave_impl_block::<MeteredWeave>(attrs, item)?;

    let impl_block = &woven_impl_block.woven_block;
    let metered = &woven_impl_block.main_attributes.to_metered();
    let measured = &woven_impl_block.woven_fns;
    let registry_name = &metered.registry_name;
    let registry_ident = &metered.registry_ident;

    let mut code = quote! {};

    let mut reg_fields = quote! {};
    for (fun_name, _) in measured.iter() {
        use heck::CamelCase;
        let fun_reg_name = format!("{}{}", registry_name, fun_name.to_string().to_camel_case());
        let fun_registry_ident = syn::Ident::new(&fun_reg_name, impl_block.impl_token.span);

        reg_fields = quote! {
            #reg_fields
            #fun_name : #fun_registry_ident,
        }
    }

    code = quote! {
        #code

        #[derive(Debug, Default, serde::Serialize)]
        struct #registry_ident {
            #reg_fields
        }
    };

    drop(reg_fields);

    for (fun_name, measure_request_attrs) in measured.iter() {
        use heck::CamelCase;
        let fun_reg_name = format!("{}{}", registry_name, fun_name.to_string().to_camel_case());
        let fun_registry_ident = syn::Ident::new(&fun_reg_name, impl_block.impl_token.span);

        let mut fun_reg_fields = quote! {};

        for measure_req_attr in measure_request_attrs.iter() {
            let metric_requests = measure_req_attr.to_requests();

            for metric in metric_requests.iter() {
                let metric_field = metric.ident();
                let metric_type = metric.type_path();

                fun_reg_fields = quote! {
                    #fun_reg_fields
                    #metric_field : #metric_type,
                }
            }
        }

        code = quote! {
            #code

            #[derive(Debug, Default, serde::Serialize)]
            struct #fun_registry_ident {
                #fun_reg_fields
            }
        };
    }

    code = quote! {
        #impl_block

        #code
    };

    let result: TokenStream = code.into();
    println!("Result {}", result.to_string());
    Ok(result)
}

struct MeteredWeave;
impl Weave for MeteredWeave {
    type MacroAttributes = MeteredKeyValAttribute;

    fn update_fn_block(
        item_fn: &syn::ImplItemMethod,
        main_attr: &Self::MacroAttributes,
        fn_attr: &[Rc<<Self as ParseAttributes>::Type>],
    ) -> syn::Result<syn::Block> {
        let metered = main_attr.to_metered();
        let block = &item_fn.block;
        let ident = &item_fn.sig.ident;

        let r: proc_macro::TokenStream = measure_list(
            &metered.registry_expr,
            &ident,
            fn_attr,
            quote! { #block }.into(),
        )
        .into();

        let new_block: syn::Block = syn::parse(r).expect("block");
        Ok(new_block)
    }
}
impl ParseAttributes for MeteredWeave {
    type Type = MeasureRequestAttribute;

    /*const*/
    fn fn_attr_name() -> &'static str {
        "measure"
    }
}

fn measure_list(
    registry_expr: &syn::Expr,
    fun_ident: &syn::Ident,
    measure_request_attrs: &[Rc<MeasureRequestAttribute>],
    expr: TokenStream,
) -> TokenStream {
    let mut inner: proc_macro2::TokenStream = expr.into();

    // Recursive macro invocations
    for measure_req_attr in measure_request_attrs.iter() {
        let metric_requests = measure_req_attr.to_requests();

        for metric in metric_requests.iter() {
            let metric_var = metric.ident();
            inner = quote! {
                metered::measure! { #metric_var, #inner }
            };
        }
    }

    // Let-bindings to avoid moving issues
    for measure_req_attr in measure_request_attrs.iter() {
        let metric_requests = measure_req_attr.to_requests();

        for metric in metric_requests.iter() {
            let metric_var = syn::Ident::new(&metric.field_name, proc_macro2::Span::call_site());

            inner = quote! {
                let #metric_var = &#registry_expr.#fun_ident.#metric_var;
                #inner
            };
        }

        // // Use debug routine if enabled!
        // if let Some(opt) = metric.debug {
        // }
    }

    // Add final braces
    inner = quote! {
        {
            #inner
        }
    };

    inner.into()
}
