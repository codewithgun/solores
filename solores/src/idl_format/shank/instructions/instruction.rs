use heck::{ToPascalCase, ToShoutySnakeCase, ToSnakeCase};
use itertools::Itertools;
use proc_macro2::{Ident, Span, TokenStream};
use quote::{format_ident, quote, ToTokens};
use serde::Deserialize;
use syn::{LitBool, LitInt};

use crate::{idl_format::shank::typedefs::TypedefField, utils::unique_by_report_dups};

#[derive(Deserialize)]
pub struct NamedInstruction {
    pub name: String,
    pub accounts: Vec<IxAccount>,
    pub args: Vec<TypedefField>,
    pub discriminant: Discriminant,
}

impl NamedInstruction {
    pub fn ix_args_ident(&self) -> Ident {
        format_ident!("{}IxArgs", &self.name.to_pascal_case())
    }

    pub fn discm_ident(&self) -> Ident {
        format_ident!("{}_IX_DISCM", &self.name.to_shouty_snake_case())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IxAccount {
    pub name: String,
    pub is_mut: bool,
    pub is_signer: bool,
    pub desc: Option<String>,
}

#[derive(Deserialize)]
pub struct Discriminant {
    pub r#type: String,
    pub value: u8,
}

impl ToTokens for NamedInstruction {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let name = &self.name;
        let accounts_ident = format_ident!("{}Accounts", name);
        let keys_ident = format_ident!("{}Keys", name);
        let ix_args_ident = self.ix_args_ident();
        let ix_data_ident = format_ident!("{}IxData", name);
        let snake_case_name = name.to_snake_case();
        let ix_fn_ident = format_ident!("{}_ix", snake_case_name);
        let invoke_fn_ident = format_ident!("{}_invoke", snake_case_name);
        let invoke_signed_fn_ident = format_ident!("{}_invoke_signed", snake_case_name);
        let verify_account_keys_fn_ident = format_ident!("{}_verify_account_keys", snake_case_name);
        let shouty_snake_case_name = name.to_shouty_snake_case();
        let accounts_len_ident = format_ident!("{}_IX_ACCOUNTS_LEN", shouty_snake_case_name);
        let discm_ident = self.discm_ident();

        let accounts = &self.accounts;
        let n_accounts = accounts.len();

        let accounts_dedup = unique_by_report_dups(accounts.iter(), |acc| acc.name.clone());

        if !accounts_dedup.duplicates.is_empty() {
            log::warn!(
                "Found duplicate accounts for instruction {}: {}. Assuming different indexes in generated AccountInfo/Meta arrays refer to the same account",
                &self.name, accounts_dedup.duplicates.iter().map(|acc| &acc.name).format(", ")
            );
        }
        let unique_accounts = &accounts_dedup.unique;

        // export accounts_len as const
        let n_accounts_lit = LitInt::new(&n_accounts.to_string(), Span::call_site());
        tokens.extend(quote! {
            pub const #accounts_len_ident: usize = #n_accounts_lit;
        });

        // impl Accounts
        let accounts_fields = unique_accounts.iter().map(|acc| {
            let account_name = format_ident!("{}", &acc.name.to_snake_case());
            let maybe_doc_comment = acc.desc.as_ref().map_or(quote! {}, |desc| {
                quote! {
                    #[doc = #desc]
                }
            });
            quote! {
                #maybe_doc_comment
                pub #account_name: &'me AccountInfo<'info>
            }
        });
        tokens.extend(quote! {
            #[derive(Copy, Clone, Debug)]
            pub struct #accounts_ident<'me, 'info> {
                #(#accounts_fields),*
            }
        });

        // impl Keys
        let keys_fields = unique_accounts.iter().map(|acc| {
            let account_ident = format_ident!("{}", &acc.name.to_snake_case());
            let maybe_doc_comment = acc.desc.as_ref().map_or(quote! {}, |desc| {
                quote! {
                    #[doc = #desc]
                }
            });
            quote! {
                #maybe_doc_comment
                pub #account_ident: Pubkey
            }
        });
        tokens.extend(quote! {
            #[derive(Copy, Clone, Debug)]
            pub struct #keys_ident {
                #(#keys_fields),*
            }
        });

        // impl From &Accounts for Keys
        let from_keys_fields = unique_accounts.iter().map(|acc| {
            let account_ident = format_ident!("{}", &acc.name.to_snake_case());
            quote! {
                #account_ident: *accounts.#account_ident.key
            }
        });
        tokens.extend(quote! {
            impl From<&#accounts_ident<'_, '_>> for #keys_ident {
                fn from(accounts: &#accounts_ident) -> Self {
                    Self {
                        #(#from_keys_fields),*
                    }
                }
            }
        });

        // impl From &Keys for [AccountMeta]
        let from_keys_meta = accounts.iter().map(|acc| acc.to_keys_account_meta_tokens());
        tokens.extend(quote! {
            impl From<&#keys_ident> for [AccountMeta; #accounts_len_ident] {
                fn from(keys: &#keys_ident) -> Self {
                    [
                        #(#from_keys_meta),*
                    ]
                }
            }
        });

        // impl From [Pubkey] for Keys
        let from_pubkey_arr_fields = unique_accounts.iter().enumerate().map(|(i, acc)| {
            let account_ident = format_ident!("{}", &acc.name.to_snake_case());
            let index_lit = LitInt::new(&i.to_string(), Span::call_site());
            quote! {
                #account_ident: pubkeys[#index_lit]
            }
        });
        tokens.extend(quote! {
            impl From<[Pubkey; #accounts_len_ident]> for #keys_ident {
                fn from(pubkeys: [Pubkey; #accounts_len_ident]) -> Self {
                    Self {
                        #(#from_pubkey_arr_fields),*
                    }
                }
            }
        });

        // impl From Accounts for [AccountInfo]
        let account_info_clone = accounts.iter().map(|acc| {
            let account_ident = format_ident!("{}", &acc.name.to_snake_case());
            quote! {
               accounts.#account_ident.clone()
            }
        });
        tokens.extend(quote! {
            impl<'info> From<&#accounts_ident<'_, 'info>> for [AccountInfo<'info>; #accounts_len_ident] {
                fn from(accounts: &#accounts_ident<'_, 'info>) -> Self {
                    [
                        #(#account_info_clone),*
                    ]
                }
            }
        });

        // impl From [AccountInfo] for Accounts
        let from_account_info_fields = accounts.iter().enumerate().map(|(i, acc)| {
            let account_ident = format_ident!("{}", &acc.name.to_snake_case());
            let index_lit = LitInt::new(&i.to_string(), Span::call_site());
            quote! {
               #account_ident: &arr[#index_lit]
            }
        });
        tokens.extend(quote! {
            impl<'me, 'info> From<&'me [AccountInfo<'info>; #accounts_len_ident]> for #accounts_ident<'me, 'info> {
                fn from(arr: &'me [AccountInfo<'info>; #accounts_len_ident]) -> Self {
                    Self {
                        #(#from_account_info_fields),*
                    }
                }
            }
        });

        // impl Args
        let args_fields = self.args.iter().map(|a| quote! { pub #a });
        tokens.extend(quote! {
            #[derive(BorshDeserialize, BorshSerialize, Clone, Debug, PartialEq)]
            #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
            pub struct #ix_args_ident {
                #(#args_fields),*
            }
        });

        // impl IxData
        let discm_value = self.discriminant.value;
        tokens.extend(quote! {
            #[derive( Clone, Debug, PartialEq)]
            pub struct #ix_data_ident(pub #ix_args_ident);

            pub const #discm_ident: u8 = #discm_value;

            impl From<#ix_args_ident> for #ix_data_ident {
                fn from(args: #ix_args_ident) -> Self {
                    Self(args)
                }
            }

            impl BorshSerialize for #ix_data_ident {
                fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
                    writer.write_all(&[#discm_ident])?;
                    self.0.serialize(writer)
                }
            }

            impl #ix_data_ident {
                pub fn deserialize(buf: &mut &[u8]) -> std::io::Result<Self> {
                    let maybe_discm = u8::deserialize(buf)?;
                    if maybe_discm != #discm_ident {
                        return Err(
                            std::io::Error::new(
                                std::io::ErrorKind::Other, format!("discm does not match. Expected: {:?}. Received: {:?}", #discm_ident, maybe_discm)
                            )
                        );
                    }
                    Ok(Self(#ix_args_ident::deserialize(buf)?))
                }
            }
        });

        // impl _ix()
        tokens.extend(quote! {
            pub fn #ix_fn_ident<K: Into<#keys_ident>, A: Into<#ix_args_ident>>(
                accounts: K,
                args: A,
            ) -> std::io::Result<Instruction> {
                let keys: #keys_ident = accounts.into();
                let metas: [AccountMeta; #accounts_len_ident] = (&keys).into();
                let args_full: #ix_args_ident = args.into();
                let data: #ix_data_ident = args_full.into();
                Ok(Instruction {
                    program_id: crate::ID,
                    accounts: Vec::from(metas),
                    data: data.try_to_vec()?,
                })
            }
        });

        // impl _invoke()
        tokens.extend(quote! {
            pub fn #invoke_fn_ident<'info, A: Into<#ix_args_ident>>(
                accounts: &#accounts_ident<'_, 'info>,
                args: A,
            ) -> ProgramResult {
                let ix = #ix_fn_ident(accounts, args)?;
                let account_info: [AccountInfo<'info>; #accounts_len_ident] = accounts.into();
                invoke(&ix, &account_info)
            }
        });

        // impl _invoke_signed()
        tokens.extend(quote! {
            pub fn #invoke_signed_fn_ident<'info, A: Into<#ix_args_ident>>(
                accounts: &#accounts_ident<'_, 'info>,
                args: A,
                seeds: &[&[&[u8]]],
            ) -> ProgramResult {
                let ix = #ix_fn_ident(accounts, args)?;
                let account_info: [AccountInfo<'info>; #accounts_len_ident] = accounts.into();
                invoke_signed(&ix, &account_info, seeds)
            }
        });

        // impl _verify_account_keys()
        let key_tups = self
            .accounts
            .iter()
            .map(IxAccount::to_verify_account_keys_tuple);
        tokens.extend(quote! {
            pub fn #verify_account_keys_fn_ident(
                accounts: &#accounts_ident<'_, '_>,
                keys: &#keys_ident
            ) -> Result<(), (Pubkey, Pubkey)> {
                for (actual, expected) in [
                    #(#key_tups),*
                ] {
                    if actual != expected {
                        return Err((*actual, *expected));
                    }
                }
                Ok(())
            }
        })
    }
}

impl IxAccount {
    pub fn field_ident(&self) -> Ident {
        format_ident!("{}", self.name.to_snake_case())
    }

    pub fn to_keys_account_meta_tokens(&self) -> TokenStream {
        let call_ident = format_ident!(
            "{}",
            match self.is_mut {
                true => "new",
                false => "new_readonly",
            }
        );
        let is_signer_arg = LitBool::new(self.is_signer, Span::call_site());
        let name = self.field_ident();
        quote! {
            AccountMeta::#call_ident(keys.#name, #is_signer_arg)
        }
    }

    pub fn to_verify_account_keys_tuple(&self) -> TokenStream {
        let name = self.field_ident();
        quote! {
            (accounts.#name.key, &keys.#name)
        }
    }
}
