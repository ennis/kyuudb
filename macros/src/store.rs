use crate::store::Multiplicity::{Many, One, ZeroOrOne};
use crate::CRATE;
use log::error;
use proc_macro2::TokenStream;
use quote::{format_ident, quote, ToTokens, TokenStreamExt};
use std::cmp;
use std::cmp::{max, min};
use std::collections::{HashMap, HashSet};
use syn::{
    braced, parenthesized,
    parse::{Parse, ParseStream},
    parse_macro_input, parse_quote,
    punctuated::Punctuated,
    spanned::Spanned,
    token,
    token::{Brace, Paren, Token},
    AngleBracketedGenericArguments, Block, Data, Error, ExprClosure, GenericParam, Generics, Ident,
    LitStr, ParenthesizedGenericArguments, Pat, PatIdent, PatTuple, PatWild, Path, Signature,
    Token, Visibility,
};

mod kw {
    syn::custom_keyword!(rel);
    syn::custom_keyword!(store);
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
enum Multiplicity {
    // '?'
    ZeroOrOne,
    // nothing
    One,
    // '*'
    Many,
}

impl Parse for Multiplicity {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.peek(Token![?]) {
            let _: Token![?] = input.parse()?;
            Ok(Multiplicity::ZeroOrOne)
        } else if input.peek(Token![*]) {
            let _: Token![*] = input.parse()?;
            Ok(Multiplicity::Many)
        } else {
            Ok(Multiplicity::One)
        }
    }
}

/// An attribute in an entity definition (e.g. `name: String`).
struct Attr {
    name: syn::Ident,
    ty: syn::Type,
}

impl Parse for Attr {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let name = input.parse()?;
        let _: Token![:] = input.parse()?;
        let ty = input.parse()?;
        Ok(Attr { name, ty })
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
enum DeleteRule {
    Deny,
    Nullify,
    Cascade,
}

/// A relationship in an entity definition (e.g. `rel album: Album?.tracks`).
#[derive(Debug, Eq, PartialEq, Hash)]
struct Rel {
    /// The name of the relationship (e.g. `album`).
    name: Ident,
    /// The destination entity type (e.g. `Album`).
    destination: Ident,
    /// The multiplicity of the relationship (e.g. `?`).
    multiplicity: Multiplicity,
    /// The inverse relationship (e.g. `tracks`).
    inverse: Option<Ident>,
    /// Delete rule
    delete_rule: Option<DeleteRule>,
}

impl Parse for Rel {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let _: kw::rel = input.parse()?;
        let name = input.parse()?;
        let _: Token![:] = input.parse()?;
        let destination = input.parse()?;
        let multiplicity = input.parse()?;
        let inverse = if input.peek(Token![.]) {
            let _: Token![.] = input.parse()?;
            Some(input.parse()?)
        } else {
            None
        };
        Ok(Rel {
            name,
            destination,
            multiplicity,
            inverse,
            delete_rule: None,
        })
    }
}

impl Rel {
    /// Returns the destination entity type in lower case.
    fn destination_lower(&self) -> Ident {
        format_ident!("{}", self.destination.to_string().to_lowercase())
    }

    fn is_optional_one(&self) -> bool {
        match self.multiplicity {
            Multiplicity::ZeroOrOne => true,
            _ => false,
        }
    }

    fn inverse<'a>(&self, store: &'a Store) -> Option<&'a Rel> {
        self.inverse.as_ref().and_then(|name| {
            store
                .entity_by_name(&self.destination)
                .ok()
                .and_then(|e| e.rel_by_name(name).ok())
        })
    }
}

/// An `Attr` or `Rel` field in an entity definition.
enum AttrOrRel {
    Attr(Attr),
    Rel(Rel),
}

impl Parse for AttrOrRel {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.peek(kw::rel) {
            Ok(AttrOrRel::Rel(input.parse()?))
        } else {
            Ok(AttrOrRel::Attr(input.parse()?))
        }
    }
}

/// The definition of an entity in the store.
///
/// # Example:
/// ```ignore
/// Album(
///     name: String,
///     rel tracks: Track*.album
/// );
/// ```
struct Entity {
    /// The name of the entity.
    name: Ident,
    /// Attributes and relationships.
    items: Punctuated<AttrOrRel, Token![,]>,
}

impl Entity {
    /// Returns an iterator over the attributes of the entity
    fn attrs(&self) -> impl Iterator<Item = &Attr> {
        self.items.iter().filter_map(|item| match item {
            AttrOrRel::Attr(attr) => Some(attr),
            _ => None,
        })
    }

    /// Returns an iterator over the relationships of the entity
    fn rels(&self) -> impl Iterator<Item = &Rel> {
        self.items.iter().filter_map(|item| match item {
            AttrOrRel::Rel(rel) => Some(rel),
            _ => None,
        })
    }

    fn rel_by_name(&self, name: &Ident) -> Result<&Rel, syn::Error> {
        self.rels().find(|r| &r.name == name).ok_or(Error::new(
            name.span(),
            format!("relation `{}` not found", name),
        ))
    }
}

impl Parse for Entity {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let name = input.parse()?;
        let content;
        parenthesized!(content in input);
        let items = Punctuated::parse_terminated(&content)?;
        // ends with a semicolon
        let _: Token![;] = input.parse()?;
        Ok(Entity { name, items })
    }
}

/// Definition of a store.
///
/// # Example:
/// ```ignore
/// pub store TrackDb;
/// Album(
///    name: String,
///    rel tracks: Track*.album   // one side of a relationship: one-to-many
/// );
/// ```
struct Store {
    /// Optional visibility.
    vis: Visibility,
    /// The name of the store. Declared with `store Name;`.
    name: Ident,
    /// Entity definitions.
    entities: Vec<Entity>,
}

impl Parse for Store {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        // Parse the `store Name;` directive.
        let vis = input.parse()?;
        let _: kw::store = input.parse()?;
        let name = input.parse()?;
        let _: Token![;] = input.parse()?;

        // Parse the entity definitions.
        let mut entities = vec![];
        while !input.is_empty() {
            entities.push(input.parse()?);
        }
        Ok(Store {
            vis,
            name,
            entities,
        })
    }
}

impl Store {
    fn entity_by_name(&self, name: &Ident) -> Result<&Entity, syn::Error> {
        self.entities
            .iter()
            .find(|e| &e.name == name)
            .ok_or(Error::new(
                name.span(),
                format!("entity `{}` not found", name),
            ))
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////
// CODEGEN

/// Two-sided relation
#[derive(Clone, Copy, Debug, PartialEq)]
struct RelEdge<'a> {
    a: &'a Rel,
    b: &'a Rel,
}

///
fn generate_entity_store_impls(
    store: &Store,
    entity: &Entity,
    out_tokens: &mut TokenStream,
    out_entity_impl: &mut TokenStream,
) -> Result<(), Error> {

    let mut checks_insert = vec![];
    let mut checks_remove = vec![];
    let mut upkeeps_insert = vec![];
    let mut upkeeps_remove = vec![];

    let is_empty = quote! {#CRATE::RelOps::is_empty};
    let is_full = quote! {#CRATE::RelOps::is_full};
    let insert = quote! {#CRATE::RelOps::insert};
    let remove = quote! {#CRATE::RelOps::remove};

    // validate relations
    for rel in entity.rels() {
        let inv_rel = rel.inverse(store);
        let rd = &rel.name;

        let check_insert;
        let check_remove;
        let upkeep_insert;
        let upkeep_remove;

        if let Some(inv_rel) = inv_rel {
            let rs = &inv_rel.name;

            match (rel.multiplicity, inv_rel.multiplicity) {
                (ZeroOrOne, ZeroOrOne | Many) => {
                    // insertion
                    check_insert = quote! {
                        let #rd = data.#rd;
                        if let Some(d) = #rd {
                            if #is_full(&self[d].#rs) {
                                return Err(#CRATE::Error::RelationshipTooManyTargets);
                            }
                        }
                    };

                    upkeep_insert = quote! {
                        if let Some(d) = #rd {
                            #insert(&mut self[d].#rs, __s);
                        }
                    };
                }
                (ZeroOrOne | One, One) => {
                    return Err(Error::new(
                        rel.name.span(),
                        "can't have a one-to-one relation with a required destination",
                    ));
                }
                (One, ZeroOrOne | Many) => {
                    check_insert = quote! {
                        let #rd = data.#rd;
                        if #is_full(&self[#rd].#rs) {
                            return Err(#CRATE::Error::RelationshipTooManyTargets);
                        }
                    };

                    upkeep_insert = quote! {
                        #insert(&mut self[#rd].#rs, __s);
                    };
                }
                _ => {
                    // TODO
                    //panic!("unimplemented: multiplicity {:?}", (rel.multiplicity, inv_rel.multiplicity));
                    continue;
                }
            }

            let delete_rule = rel.delete_rule.unwrap_or(match inv_rel.multiplicity {
                ZeroOrOne => DeleteRule::Nullify,
                One => DeleteRule::Deny,
                Many => DeleteRule::Nullify,
            });

            // Can't nullify if the destination is required
            if delete_rule == DeleteRule::Nullify && inv_rel.multiplicity == One {
                return Err(Error::new(
                    rel.name.span(),
                    "can't nullify a required relation",
                ));
            }

            match (rel.multiplicity, delete_rule, inv_rel.multiplicity) {
                (ZeroOrOne | Many, DeleteRule::Deny, _) => {
                    check_remove = quote! {
                        if !#is_empty(&self[s].#rd) {
                            return Err(#CRATE::Error::RelationshipDeniedDelete);
                        }
                    };
                    upkeep_remove = quote! {};
                }
                (One, DeleteRule::Deny, _) => {
                    check_remove = quote! { return Err(#CRATE::Error::RelationshipDeniedDelete); };
                    upkeep_remove = quote! {};
                }

                (ZeroOrOne | Many, DeleteRule::Nullify, ZeroOrOne | Many) => {
                    check_remove = quote! {};
                    upkeep_remove = quote! {
                        for d in self[s].#rd {
                            #remove(&mut self[d].#rs, s);
                        }
                    };
                }
                (One, DeleteRule::Nullify, ZeroOrOne | Many) => {
                    check_remove = quote! {};
                    upkeep_remove = quote! {
                        let d = data.#rd;
                        #remove(&mut self[d].#rs, s);
                    };
                }

                // nullify but the destination is non-nullable
                (_, DeleteRule::Nullify, One) => {
                    check_remove = quote! { return Err(#CRATE::Error::RelationshipTooFewTargets); };
                    upkeep_remove = quote! {};
                }

                (ZeroOrOne | Many, DeleteRule::Cascade, _) => {
                    check_remove = quote! {
                        for d in self[s].#rd {
                            self.check_remove(d)?;
                        }
                    };
                    upkeep_remove = quote! {
                        for d in data.#rd {
                            self.remove_unchecked(d);
                        }
                    };
                }
                (One, DeleteRule::Cascade, _) => {
                    check_remove = quote! {
                        let d = self[s].#rd;
                        self.check_remove(d)?;
                    };
                    upkeep_remove = quote! {
                        self.remove_unchecked(data.#rd);
                    };
                }
            }

            checks_insert.push(check_insert);
            checks_remove.push(check_remove);
            upkeeps_insert.push(upkeep_insert);
            upkeeps_remove.push(upkeep_remove);
        }
    }

    let entity_ty = &entity.name;
    let row_ty = format_ident!("{}Row", entity_ty);
    let store_ty = format_ident!("{}Store", store.name);

    let vis = &store.vis;
    let db_name = &store.name;
    let fields = entity.items.iter().map(|item| match item {
        AttrOrRel::Attr(attr) => {
            let name = &attr.name;
            let ty = &attr.ty;
            quote! {#name: #ty,}
        }
        AttrOrRel::Rel(rel) => {
            let name = &rel.name;
            let ty = &rel.destination;
            // TODO: let the user choose the container
            match rel.multiplicity {
                Multiplicity::ZeroOrOne => {
                    quote! {#name: Option<#ty>,}
                }
                Multiplicity::One => {
                    quote! {#name: #ty,}
                }
                Multiplicity::Many => {
                    quote! {#name: Vec<#ty>,}
                }
            }
        }
    });

    let entity_store = quote!{#CRATE::EntityStore::<#entity_ty>};

    out_tokens.append_all(quote!{

        #[derive(Copy, Clone, Default, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
        #[repr(transparent)]
        #vis struct #entity_ty(#CRATE::slotmap::KeyData);
        impl From<#CRATE::slotmap::KeyData> for #entity_ty {
            fn from(k: #CRATE::slotmap::KeyData) -> Self {
                #entity_ty(k)
            }
        }
        unsafe impl #CRATE::slotmap::Key for #entity_ty {
            fn data(&self) -> #CRATE::slotmap::KeyData {
                self.0
            }
        }

        #vis struct #row_ty {
            #(#fields)*
        }

        impl #row_ty {
            #vis fn insert(self, db: &mut dyn #db_name) -> Result<#entity_ty, #CRATE::Error> {
                #entity_store::insert(db.store_mut(), self)
            }
        }

        impl ::std::ops::Index<#entity_ty> for #store_ty {
            type Output = #row_ty;
            fn index(&self, idx: #entity_ty) -> &Self::Output {
                &self.#entity_ty[idx]
            }
        }

        impl ::std::ops::IndexMut<#entity_ty> for #store_ty {
            fn index_mut(&mut self, idx: #entity_ty) -> &mut Self::Output {
                &mut self.#entity_ty[idx]
            }
        }

        impl #CRATE::Entity for #entity_ty {
            type Row = #row_ty;
        }


        impl #CRATE::EntityStore<#entity_ty> for #store_ty {
            fn insert(&mut self, data: #row_ty) -> Result<#entity_ty, #CRATE::Error> {
                #(#checks_insert)*
                let __s = self.#entity_ty.insert(data);
                #(#upkeeps_insert)*
                Ok(__s)
            }
            fn check_remove(&self, s: #entity_ty) -> Result<(), #CRATE::Error> {
                #(#checks_remove)*
                Ok(())
            }
            fn remove(&mut self, s: #entity_ty) -> Result<#row_ty, #CRATE::Error> {
                self.check_remove(s)?;
                let data = self.remove_unchecked(s);
                #(#upkeeps_remove)*
                Ok(data)
            }
            fn remove_unchecked(&mut self, index: #entity_ty) -> #row_ty {
                self.#entity_ty.remove(index).expect("invalid index")
            }
        }
    });

    out_entity_impl.append_all(quote! {
        #vis fn remove(self, db: &mut dyn #db_name) -> Result<#row_ty, #CRATE::Error> {
            #entity_store::remove(db.store_mut(), self)
        }

        #vis fn data(self, db: &dyn #db_name) -> &#row_ty {
            &db.store().#entity_ty[self]
        }
    });

    // Getters & setters
    for item in entity.items.iter() {
        match item {
            AttrOrRel::Attr(attr) => {
                let name = &attr.name;
                let setter = format_ident!("set_{}", name);
                let ty = &attr.ty;
                out_entity_impl.append_all(quote! {
                    #vis fn #name (self, db: &dyn #db_name) -> &#ty {
                        &db.store().#entity_ty[self].#name
                    }

                    #vis fn #setter (self, db: &mut dyn #db_name, value: #ty) {
                        db.store_mut().#entity_ty[self].#name = value;
                    }
                });
            }
            AttrOrRel::Rel(rel) => {
                let name = &rel.name;
                let ty = &rel.destination;
                out_entity_impl.append_all(match rel.multiplicity {
                    ZeroOrOne => quote! {
                        #vis fn #name (self, db: &dyn #db_name) -> Option<#ty> {
                            db.store().#entity_ty[self].#name
                        }
                    },
                    One => quote! {
                        #vis fn #name (self, db: &dyn #db_name) -> #ty {
                            db.store().#entity_ty[self].#name
                        }
                    },
                    Many => quote! {
                        #vis fn #name<'a>(self, db: &'a dyn #db_name) -> &'a [#ty] {
                            &db.store().#entity_ty[self].#name
                        }
                    },
                });
            }
        }
    }

    Ok(())
}

/// Generates the method (a tuple `(signature, implementation)`) that sets the specified relation.
///
/// # Example
/// For the relation `rel tracks: Track*.album` in `Album`, this will generate the following method:
/// ```ignore
/// fn add_album_tracks(&mut self, src: Album, dst: Track) -> Result<(), Error>;
/// ```
///
/// Inversely, for `rel album: Album.tracks` in `Track`:
/// ```ignore
/// fn set_track_album(&mut self, src: Track, dst: Album) -> Result<(), Error>;
/// ```
///
fn generate_add_rel_method(
    store: &Store,
    src: &Entity,
    rel: &Rel,
    out_tokens: &mut TokenStream,
    out_entity_methods: &mut TokenStream,
) -> Result<(), Error> {
    let inv_rel = rel.inverse(store);
    let rd = &rel.name;
    use Multiplicity::*;

    let is_empty = quote! {#CRATE::RelOps::is_empty};
    let is_full = quote! {#CRATE::RelOps::is_full};
    let insert = quote! {#CRATE::RelOps::insert};
    let remove = quote! {#CRATE::RelOps::remove};

    let stmts = if let Some(inv_rel) = inv_rel {
        let rs = &inv_rel.name;
        match (rel.multiplicity, inv_rel.multiplicity) {
            (ZeroOrOne, ZeroOrOne | Many) => {
                quote! {
                    if let Some(d) = d {
                        if #is_full(&self[d].#rs) {
                            return Err(#CRATE::Error::RelationshipTooManyTargets);
                        }
                    }
                    if let Some(d0) = self[s].#rd {
                        #remove(&mut self[d0].#rs, s);
                    }
                    self[s].#rd = d;
                    if let Some(d) = d {
                        #insert(&mut self[d].#rs, s);
                    }
                }
            }
            (ZeroOrOne, One) => {
                // deny: this would always break the existing relation from D to S0
                return Ok(());
            }
            (One, One) => {
                // impossible
                return Ok(());
            }
            (One, ZeroOrOne | Many) => {
                quote! {
                    if #is_full(&self[d].#rs) {
                        return Err(#CRATE::Error::RelationshipTooManyTargets);
                    }
                    let d0 = self[s].#rd;
                    #remove(&mut self[d0].#rs, s);
                    self[s].#rd = d;
                    #insert(&mut self[d].#rs, s);
                }
            }
            (Many, Many) => {
                // TODO
                return Ok(());
            }
            (One, ZeroOrOne) | (Many, ZeroOrOne) | (Many, One) => {
                // Use the other side
                return Ok(());
            }
        }
    } else {
        match rel.multiplicity {
            ZeroOrOne | One => quote! { self[s].#rd = d; },
            Many => quote! { #insert(&mut self[s].#rd, d); },
        }
    };

    let store_name = format_ident!("{}Store", store.name);
    let db_name = &store.name;

    let rel_verb = match rel.multiplicity {
        One | ZeroOrOne => "set",
        Many => "add",
    };
    let rel_name = format_ident!(
        "{}_{}_{}",
        rel_verb,
        src.name.to_string().to_lowercase(),
        rel.name.to_string().to_lowercase()
    );
    let t_src = &src.name;
    let t_dst = if rel.is_optional_one() {
        let ty = &rel.destination;
        quote!(Option<#ty>)
    } else {
        let ty = &rel.destination;
        quote!(#ty)
    };

    out_tokens.append_all(quote! {
        impl #store_name {
            fn #rel_name (&mut self, s: #t_src, d: #t_dst) -> Result<(), #CRATE::Error> {
                #stmts
                Ok(())
            }
        }
    });

    let entity_rel_name = format_ident!("{}_{}", rel_verb, rel.name.to_string().to_lowercase());

    out_entity_methods.append_all(quote! {
        fn #entity_rel_name (self, db: &mut dyn #db_name, dst: #t_dst) -> Result<(), #CRATE::Error> {
            db.store_mut().#rel_name(self, dst)
        }
    });

    Ok(())
}

pub(crate) fn generate_store(input: proc_macro::TokenStream) -> syn::Result<TokenStream> {
    let store: Store = syn::parse(input)?;

    /*// Build the graph of relations
    let mut rel_edges = HashMap::new();

    for entity in store.entities.iter() {
        for src_rel in entity.rels() {
            if let Some(ref inverse) = src_rel.inverse {
                // find the destination entity & relation
                let Some(dst) = store
                    .entities
                    .iter()
                    .find(|e| e.name == src_rel.destination)
                else {
                    errors.push(
                        Error::new(src_rel.destination.span(), "destination entity not found")
                            .into_compile_error(),
                    );
                    continue;
                };
                let Some(dst_rel) = dst.rels().find(|r| &r.name == inverse)
                else {
                    errors.push(
                        Error::new(inverse.span(), "inverse relation not found")
                            .into_compile_error(),
                    );
                    continue;
                };

                // we'll see both sides of the relation (a -> b and b -> a)
                // we want only one entry in the hash map, so we use a key that's the same for both using their addresses
                // `(min(a_ptr, b_ptr), max(a_ptr, b_ptr))`
                let key = (min(src_rel as *const _, dst_rel as *const _), max(src_rel as *const _, dst_rel as *const _));

                rel_edges.insert(key, RelEdge {
                    a: src_rel,
                    b: dst_rel,
                });
            }
        }
    }*/

    // name of the wrapper trait (e.g. `MusicDb`)
    let trait_name = &store.name;
    // name of the struct that stores the data (e.g. `MusicDbStore`)
    let store_name = format_ident!("{}Store", trait_name);

    // generate code for each entity
    let mut tokens = TokenStream::new();
    for entity in store.entities.iter() {
        let mut entity_impl = TokenStream::new();
        generate_entity_store_impls(&store, entity, &mut tokens, &mut entity_impl)?;
        // generate relation methods
        for rel in entity.rels() {
            generate_add_rel_method(&store, entity, rel, &mut tokens, &mut entity_impl)?;
        }
        let name = &entity.name;
        tokens.append_all(quote! {
            impl #name {
                #entity_impl
            }
        });
    }

    let entity_names = store.entities.iter().map(|entity| &entity.name);
    let vis = &store.vis;

    let store_fields = store.entities.iter().map(|entity| {
        let name = &entity.name;
        let row_name = format_ident!("{}Row", name);
        quote! {#name : #CRATE::slotmap::SlotMap<#name, #row_name>,}
    });

    let crate_ = &CRATE;
    let code = quote! {


        #[derive(Default)]
        #vis struct #store_name {
            #(#store_fields)*
        }

        impl #store_name {
            #vis fn new() -> #store_name {
                Self::default()
            }
        }

        #tokens

        #vis trait #trait_name: #CRATE::HasStore<#store_name> {}
        impl<DB> #trait_name for DB where DB: #CRATE::HasStore<#store_name> {}
    };

    Ok(code)
}
