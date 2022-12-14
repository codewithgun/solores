use quote::quote;

use crate::{idl_format::IdlFormat, Args};

use super::write_src_file;

pub fn write_lib(args: &Args, idl: &dyn IdlFormat) -> std::io::Result<()> {
    let id = idl.program_address();

    let mut contents = quote! {
        solana_program::declare_id!(#id);
    };

    if idl.has_accounts() {
        contents.extend(quote! {
            pub mod accounts;
            pub use accounts::*;
        })
    }

    if idl.has_instructions() {
        contents.extend(quote! {
            pub mod instructions;
            pub use instructions::*;
        })
    }

    if idl.has_typedefs() {
        contents.extend(quote! {
            pub mod typedefs;
            pub use typedefs::*;
        })
    }

    write_src_file(args, "src/lib.rs", contents)
}
