use proc_macro::TokenStream;
use quote::ToTokens;
use syn::parse_quote;

#[proc_macro]
pub fn enhance(tokens: TokenStream) -> TokenStream {
    let tokens = for_all_items(tokens, add_event_enum);
    let tokens = for_all_items(tokens, add_conversions);
    tokens
}

fn for_all_items<F>(tokens: TokenStream, func: F) -> TokenStream
where
    F: Fn(&syn::Item) -> Vec<syn::Item>,
{
    let mut file: syn::File = syn::parse(tokens).expect("Unable to parse rust source to syn::File");

    let mut modified_items = Vec::with_capacity(file.items.len());
    for item in &file.items {
        modified_items.append(&mut func(item));
    }
    file.items = modified_items;

    file.into_token_stream().into()
}

fn add_event_enum(item: &syn::Item) -> Vec<syn::Item> {
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
            modified_items.append(&mut add_event_enum(item));
        }
        item_mod.content = Some((brace, modified_items));
        return vec![syn::Item::Mod(item_mod)];
    }

    if let syn::Item::Enum(item_enum) = item {
        if !format!("{}", item_enum.ident).ends_with("ProtocolMessage") {
            return vec![syn::Item::Enum(item_enum.to_owned())];
        }

        let variants: Vec<syn::Variant> = item_enum
            .variants
            .iter()
            .filter(|variant| {
                variant
                    .attrs
                    .iter()
                    .filter_map(|attr| attr.path.segments.iter().last().map(|ident| (attr, ident)))
                    .filter(|(_, ident)| format!("{}", ident.ident) == "prost")
                    .filter_map(|(attr, _)| attr.tokens.to_owned().into_iter().last())
                    .filter_map(|token| {
                        if let proc_macro2::TokenTree::Group(group) = token {
                            Some(group)
                        } else {
                            None
                        }
                    })
                    .filter_map(|group| group.stream().into_iter().last())
                    .filter_map(|token| {
                        if let proc_macro2::TokenTree::Literal(id) = token {
                            Some(id)
                        } else {
                            None
                        }
                    })
                    .map(|id| format!("{id}"))
                    .map(|id| id.replace("\"", ""))
                    .filter_map(|id| id.parse::<usize>().ok())
                    // No StatusMessages or ServerAdvertisements
                    .filter(|id| *id >= 10)
                    // No Server-bound messages
                    .any(|id| id < 40)
            })
            .map(|x| x.to_owned())
            .collect();

        let variant_identifiers: Vec<syn::Ident> = variants
            .iter()
            .map(|variant| variant.ident.clone())
            .collect();

        let event_enum: syn::ItemEnum = parse_quote!(

            #[derive(prost::Oneof)]
            pub enum EventMessage {
                #(#variants),*
            }

        );

        let from_event_item: syn::ItemImpl = parse_quote!(

            impl From<EventMessage> for ProtocolMessage {
                fn from(event_message: EventMessage) -> Self {
                    match event_message {
                        #(
                            EventMessage::#variant_identifiers(message) => ProtocolMessage::#variant_identifiers(message),
                        )*
                    }
                }
            }

        );

        let from_protocol_item: syn::ItemImpl = parse_quote!(

            impl TryFrom<ProtocolMessage> for EventMessage {
                type Error = ();
                fn try_from(protocol_message: ProtocolMessage) -> Result<Self, Self::Error> {
                    match protocol_message {
                        #(
                            ProtocolMessage::#variant_identifiers(message) => Ok(EventMessage::#variant_identifiers(message)),
                        )*
                        _ => Err(()),
                    }
                }
            }

        );

        return vec![
            syn::Item::Enum(item_enum.to_owned()),
            syn::Item::Enum(event_enum),
            syn::Item::Impl(from_event_item),
            syn::Item::Impl(from_protocol_item),
        ];
    }
    vec![item.to_owned()]
}

fn add_conversions(item: &syn::Item) -> Vec<syn::Item> {
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
            modified_items.append(&mut add_conversions(item));
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
