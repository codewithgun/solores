use heck::{ToShoutySnakeCase, ToSnakeCase};
use itertools::Itertools;
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote, ToTokens};
use serde::Deserialize;
use syn::{Generics, Lifetime, LifetimeDef, LitBool};

use crate::utils::unique_by_report_dups;

use super::typedefs::TypedefField;

#[derive(Deserialize)]
pub struct NamedInstruction {
    pub name: String,
    pub accounts: Vec<IxAccount>,
    pub args: Vec<TypedefField>,
    pub discriminant: Discriminant,
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
        let ix_args_ident = format_ident!("{}IxArgs", name);
        let ix_data_ident = format_ident!("{}IxData", name);
        let snake_case_name = name.to_snake_case();
        let ix_fn_ident = format_ident!("{}_ix", snake_case_name);
        let invoke_fn_ident = format_ident!("{}_invoke", snake_case_name);
        let invoke_signed_fn_ident = format_ident!("{}_invoke_signed", snake_case_name);
        let shouty_snake_case_name = name.to_shouty_snake_case();
        let accounts_len_ident = format_ident!("{}_IX_ACCOUNTS_LEN", shouty_snake_case_name);
        let discm_ident = format_ident!("{}_IX_DISCM", shouty_snake_case_name);

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
        tokens.extend(quote! {
            pub const #accounts_len_ident: usize = #n_accounts;
        });

        // impl Accounts

        let anon_lifetime_def = LifetimeDef::new(Lifetime::new("'_", Span::call_site()));
        let me_lifetime_def = LifetimeDef::new(Lifetime::new("'me", Span::call_site()));
        let info_lifetime_def = LifetimeDef::new(Lifetime::new("'info", Span::call_site()));

        // <'me, 'info>
        let mut accounts_lifetimes_full = Generics::default();
        accounts_lifetimes_full
            .params
            .push(me_lifetime_def.clone().into());
        accounts_lifetimes_full
            .params
            .push(info_lifetime_def.clone().into());

        // <'_, '_>
        let mut account_lifetimes_all_anon = Generics::default();
        for _i in 0..2 {
            account_lifetimes_all_anon
                .params
                .push(anon_lifetime_def.clone().into());
        }

        // <'_, 'info>
        let mut account_lifetimes_me_anon = Generics::default();
        account_lifetimes_me_anon
            .params
            .push(anon_lifetime_def.into());
        account_lifetimes_me_anon
            .params
            .push(info_lifetime_def.clone().into());

        // <'info>
        let mut account_info_lifetime = Generics::default();
        account_info_lifetime.params.push(info_lifetime_def.into());

        let accounts_fields = unique_accounts.iter().map(|acc| {
            let account_name = format_ident!("{}", &acc.name.to_snake_case());
            let maybe_doc_comment = acc.desc.as_ref().map_or(quote! {}, |desc| {
                quote! {
                    #[doc = #desc]
                }
            });
            quote! {
                #maybe_doc_comment
                pub #account_name: &'me AccountInfo #account_info_lifetime
            }
        });
        tokens.extend(quote! {
            #[derive(Copy, Clone, Debug)]
            pub struct #accounts_ident #accounts_lifetimes_full {
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
            impl From<&#accounts_ident #account_lifetimes_all_anon> for #keys_ident {
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

        // impl From Accounts for [AccountInfo]
        let account_info_clone = accounts.iter().map(|acc| {
            let account_ident = format_ident!("{}", &acc.name.to_snake_case());
            quote! {
               accounts.#account_ident.clone()
            }
        });
        tokens.extend(quote! {
            impl<'info> From<&#accounts_ident #account_lifetimes_me_anon> for [AccountInfo<'info>; #accounts_len_ident] {
                fn from(accounts: &#accounts_ident #account_lifetimes_me_anon) -> Self {
                    [
                        #(#account_info_clone),*
                    ]
                }
            }
        });

        // impl Args
        let args_fields = self.args.iter().map(|a| quote! { pub #a });
        tokens.extend(quote! {
            #[derive(BorshDeserialize, BorshSerialize, Clone, Debug)]
            #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
            pub struct #ix_args_ident {
                #(#args_fields),*
            }
        });

        // impl IxData
        let discm_value = self.discriminant.value;
        tokens.extend(quote! {
            #[derive(Copy, Clone, Debug)]
            pub struct #ix_data_ident<'me>(pub &'me#ix_args_ident);

            pub const #discm_ident: u8 = #discm_value;

            impl<'me> From<&'me #ix_args_ident> for #ix_data_ident<'me> {
                fn from(args: &'me #ix_args_ident) -> Self {
                    Self(args)
                }
            }

            impl BorshSerialize for #ix_data_ident<'_> {
                fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
                    writer.write_all(&[#discm_ident])?;
                    self.0.serialize(writer)
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
                let data: #ix_data_ident = (&args_full).into();
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
                accounts: &#accounts_ident #account_lifetimes_me_anon,
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
                accounts: &#accounts_ident #account_lifetimes_me_anon,
                args: A,
                seeds: &[&[&[u8]]],
            ) -> ProgramResult {
                let ix = #ix_fn_ident(accounts, args)?;
                let account_info: [AccountInfo<'info>; #accounts_len_ident] = accounts.into();
                invoke_signed(&ix, &account_info, seeds)
            }
        });
    }
}

impl IxAccount {
    pub fn to_keys_account_meta_tokens(&self) -> TokenStream {
        let call_ident = format_ident!(
            "{}",
            match self.is_mut {
                true => "new",
                false => "new_readonly",
            }
        );
        let is_signer_arg = LitBool::new(self.is_signer, Span::call_site());
        let name = format_ident!("{}", self.name.to_snake_case());
        quote! {
            AccountMeta::#call_ident(keys.#name, #is_signer_arg)
        }
    }
}
