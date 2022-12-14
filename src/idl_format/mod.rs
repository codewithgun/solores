use proc_macro2::TokenStream;
use quote::{quote, ToTokens};

pub mod shank;

#[derive(Copy, Clone, Debug, Default)]
pub struct TypedefsHeaderFlags {
    pub has_pubkey: bool,
}

#[derive(Copy, Clone, Debug, Default)]
pub struct AccountsHeaderFlags {
    pub has_pubkey: bool,
    pub has_defined: bool,
}

#[derive(Copy, Clone, Debug, Default)]
pub struct InstructionsHeaderFlags {
    pub has_defined: bool,
}

trait IdlCodegenElems {
    type TypedefElem: ToTokens;
    type AccountElem: ToTokens;
    type IxElem: ToTokens;

    fn typedefs(&self) -> Option<&[Self::TypedefElem]>;

    fn accounts(&self) -> Option<&[Self::AccountElem]>;

    fn instructions(&self) -> Option<&[Self::IxElem]>;
}

pub trait IdlCodegen {
    fn typedefs_file(&self) -> Option<TokenStream>;

    fn accounts_file(&self) -> Option<TokenStream>;

    fn instructions_file(&self) -> Option<TokenStream>;

    fn has_typedefs(&self) -> bool;

    fn has_accounts(&self) -> bool;

    fn has_instructions(&self) -> bool;
}

impl<I: IdlCodegenElems> IdlCodegen for I {
    fn typedefs_file(&self) -> Option<TokenStream> {
        let elems = self.typedefs()?.iter().map(|e| e.into_token_stream());
        let mut res = quote! {};
        res.extend(elems);
        Some(res)
    }

    fn accounts_file(&self) -> Option<TokenStream> {
        let elems = self.accounts()?.iter().map(|e| e.into_token_stream());
        let mut res = quote! {};
        res.extend(elems);
        Some(res)
    }

    fn instructions_file(&self) -> Option<TokenStream> {
        let elems = self.instructions()?.iter().map(|e| e.into_token_stream());
        let mut res = quote! {};
        res.extend(elems);
        Some(res)
    }

    fn has_typedefs(&self) -> bool {
        self.typedefs().is_some()
    }

    fn has_accounts(&self) -> bool {
        self.accounts().is_some()
    }

    fn has_instructions(&self) -> bool {
        self.instructions().is_some()
    }
}

pub trait IdlFormat: IdlCodegen {
    fn program_name(&self) -> &str;

    fn program_version(&self) -> &str;

    fn program_address(&self) -> &str;

    fn is_correct_idl_format(&self) -> bool;

    fn det_typedefs_header_flags(&self) -> TypedefsHeaderFlags;

    fn det_accounts_header_flags(&self) -> AccountsHeaderFlags;

    fn det_instructions_header_flags(&self) -> InstructionsHeaderFlags;

    fn typedefs_header(&self) -> TokenStream {
        let flags = self.det_typedefs_header_flags();
        let mut res = quote! {
            use borsh::{BorshDeserialize, BorshSerialize};
        };
        if flags.has_pubkey {
            res.extend(quote! {
                use solana_program::pubkey::Pubkey;
            });
        }
        res
    }

    fn accounts_header(&self) -> TokenStream {
        let flags = self.det_accounts_header_flags();
        let mut res = quote! {
            use borsh::{BorshDeserialize, BorshSerialize};
        };
        if flags.has_pubkey {
            res.extend(quote! {
                use solana_program::pubkey::Pubkey;
            });
        }
        if flags.has_defined {
            res.extend(quote! {
                use crate::*;
            })
        }
        res
    }

    fn instructions_header(&self) -> TokenStream {
        let flags = self.det_instructions_header_flags();
        let mut res = quote! {
            use borsh::{BorshDeserialize, BorshSerialize};
            use solana_program::{
                account_info::AccountInfo,
                entrypoint::ProgramResult,
                instruction::{AccountMeta, Instruction},
                program::{invoke, invoke_signed},
                pubkey::Pubkey,
            };
        };
        if flags.has_defined {
            res.extend(quote! {
                use crate::*;
            })
        }
        res
    }
}
