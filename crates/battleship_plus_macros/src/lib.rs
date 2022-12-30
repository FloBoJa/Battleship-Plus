use proc_macro::TokenStream;
use quote::ToTokens;
use syn::parse_quote;

#[proc_macro]
pub fn add_conversions(tokens: TokenStream) -> TokenStream {
    let mut file: syn::File = syn::parse(tokens).expect("Unable to parse rust source to syn::File");

    let mut modified_items = Vec::with_capacity(file.items.len());
    for item in &file.items {
        modified_items.append(&mut process_item(item));
    }
    file.items = modified_items;

    file.into_token_stream().into()
}

fn process_item(item: &syn::Item) -> Vec<syn::Item> {
    // Modules, processed recursively.
    if let syn::Item::Mod(
        item_mod @ syn::ItemMod {
            content: Some(_), ..
        },
    ) = item
    {
        let mut item_mod = item_mod.to_owned();
        let (brace, items) = item_mod.content.unwrap();
        let mut modified_items = Vec::with_capacity(items.len());
        for item in &items {
            modified_items.append(&mut process_item(item));
        }
        item_mod.content = Some((brace, modified_items));
        return vec![syn::Item::Mod(item_mod)];
    }

    // Enums, specifically prost oneof enums are extended with try_from.
    if let syn::Item::Enum(item_enum) = item {
        if !is_oneof_enum(item_enum) {
            return vec![syn::Item::Enum(item_enum.to_owned())];
        }

        let mut result = vec![syn::Item::Enum(item_enum.to_owned())];
        let enum_ident = &item_enum.ident;
        for variant in &item_enum.variants {
            let variant_ident = &variant.ident;
            let contained_type = &variant
                .fields
                .iter()
                .next()
                .expect("oneof variants have a field")
                .to_owned()
                .ty;

            let impl_item: syn::Item = parse_quote!(

               impl TryFrom<#enum_ident> for #contained_type {
                   type Error = ();
                   fn try_from(oneof_variant: #enum_ident) -> Result<Self, Self::Error> {
                       if let #enum_ident::#variant_ident(message) = oneof_variant {
                           return Ok(message)
                       }
                       Err(())
                   }
               }

            );
            result.push(impl_item);

            let impl_item: syn::Item = parse_quote!(

               impl From<#contained_type> for #enum_ident {
                   fn from(contained_type: #contained_type) -> Self {
                       #enum_ident::#variant_ident(contained_type)
                   }
               }

            );
            result.push(impl_item);
        }
        return result;
    }

    // Do not touch anything else.
    vec![item.to_owned()]
}

fn is_oneof_enum(item_enum: &syn::ItemEnum) -> bool {
    item_enum.attrs.iter().any(|attribute| {
        attribute
            .to_owned()
            .tokens
            .into_iter()
            .filter_map(|x| {
                if let proc_macro2::TokenTree::Group(group) = x {
                    Some(group.stream())
                } else {
                    None
                }
            })
            .flatten()
            .filter_map(|x| {
                if let proc_macro2::TokenTree::Ident(ident) = x {
                    Some(format!("{ident}"))
                } else {
                    None
                }
            })
            .fold("".to_string(), |acc, x| acc + &x)
            .contains("prostOneof")
    })
}
