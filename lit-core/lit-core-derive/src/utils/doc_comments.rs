//! Copied from: https://github.com/clap-rs/clap/blob/master/clap_derive/src/utils/doc_comments.rs
//! (wasn't exported sadly).
//!
//! The preprocessing we apply to doc comments.

pub fn extract_doc_comment(attrs: &[syn::Attribute]) -> Vec<String> {
    use syn::Lit::*;
    use syn::Meta::*;
    use syn::MetaNameValue;

    // multiline comments (`/** ... */`) may have LFs (`\n`) in them,
    // we need to split so we could handle the lines correctly
    //
    // we also need to remove leading and trailing blank lines
    let mut lines: Vec<_> = attrs
        .iter()
        .filter(|attr| attr.path.is_ident("doc"))
        .filter_map(|attr| {
            if let Ok(NameValue(MetaNameValue { lit: Str(s), .. })) = attr.parse_meta() {
                Some(s.value())
            } else {
                // non #[doc = "..."] attributes are not our concern
                // we leave them for rustc to handle
                None
            }
        })
        .skip_while(|s| is_blank(s))
        .flat_map(|s| {
            s.split('\n')
                .map(|s| {
                    // remove one leading space no matter what
                    let s = s.strip_prefix(' ').unwrap_or(s);
                    s.to_owned()
                })
                .collect::<Vec<_>>()
        })
        .collect();

    while let Some(true) = lines.last().map(|s| is_blank(s)) {
        lines.pop();
    }

    lines
}

fn is_blank(s: &str) -> bool {
    s.trim().is_empty()
}
