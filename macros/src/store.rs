use std::collections::HashSet;
use proc_macro2::TokenStream;
use quote::{format_ident, quote, ToTokens, TokenStreamExt};
use syn::{parenthesized, parse::{Parse, ParseStream}, punctuated::Punctuated, spanned::Spanned, token::Token, Error, Ident, Token, Visibility, braced};

use crate::store::Multiplicity::{Many, One, ZeroOrOne};
use crate::CRATE;

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
    attrs: Vec<syn::Attribute>,
    name: syn::Ident,
    ty: syn::Type,
}

impl Parse for Attr {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let name = input.parse()?;
        let _: Token![:] = input.parse()?;
        let ty = input.parse()?;
        Ok(Attr {
            attrs: vec![],
            name,
            ty,
        })
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
enum DeleteRule {
    Deny,
    Nullify,
    Cascade,
}

/// A relationship in an entity definition (e.g. `rel album: Album?.tracks`).
struct Rel {
    attrs: Vec<syn::Attribute>,
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
    unique: bool,
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
            attrs: vec![],
            name,
            destination,
            multiplicity,
            inverse,
            delete_rule: None,
            unique: false,
        })
    }
}

impl Rel {
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

    /// Returns the index name for the relationship
    fn index_field(&self, entity: &Entity) -> Ident {
        format_ident!("index_{}_{}", entity.name, self.name)
    }

    fn foreign_key_type(&self) -> syn::Type {
        if self.is_optional_one() {
            let ty = &self.destination;
            syn::parse_quote!(Option<#ty>)
        } else {
            let ty = &self.destination;
            syn::parse_quote!(#ty)
        }
        // TODO many
    }
}

/// An `Attr` or `Rel` field in an entity definition.
enum AttrOrRel {
    Attr(Attr),
    Rel(Rel),
}

impl Parse for AttrOrRel {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let attrs = input.call(syn::Attribute::parse_outer)?;
        let mut item = if input.peek(kw::rel) {
            AttrOrRel::Rel(input.parse()?)
        } else {
            AttrOrRel::Attr(input.parse()?)
        };
        match item {
            AttrOrRel::Attr(ref mut attr) => attr.attrs = attrs,
            AttrOrRel::Rel(ref mut rel) => rel.attrs = attrs,
        };
        Ok(item)
    }
}

/// The definition of an entity in the store.
///
/// # Example:
/// ```ignore
/// Album(AlbumId) {
///     name: String,
///     rel tracks: Track*.album
/// }
/// ```
struct Entity {
    /// Attributes.
    attrs: Vec<syn::Attribute>,
    /// The name of the entity.
    name: Ident,
    keys: Punctuated<Ident, Token![,]>,
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

    fn key_ty(&self) -> syn::Type {
        if self.keys.len()  == 1 {
            let k = &self.keys[0];
            syn::parse_quote!(#k)
        } else {
            let ks = self.keys.iter();
            syn::parse_quote!((#(#ks),*))
        }
    }
}

impl Parse for Entity {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let attrs = input.call(syn::Attribute::parse_outer)?;
        let name = input.parse()?;

        let content;
        parenthesized!(content in input);
        let keys = Punctuated::parse_terminated(&content)?;

        let content;
        braced!(content in input);
        let items = Punctuated::parse_terminated(&content)?;

        Ok(Entity { attrs, keys, name, items })
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
    attrs: Vec<syn::Attribute>,
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
        let attrs = input.call(syn::Attribute::parse_outer)?;
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
            attrs,
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

    fn store_type(&self) -> syn::Type {
        let name = &self.name;
        let ty = format_ident!("{}Store", name);
        syn::parse_quote!(#ty)
    }

    /// Returns all indices on the given entity.
    fn indices_for_entity(&self, entity: &Entity) -> Vec<Ident> {
        let mut indices = HashSet::new();
        for ent in self.entities.iter() {
            for rel in ent.rels() {
                if rel.destination == entity.name {
                    indices.insert(rel.index_field(entity));
                }
            }
        }
        indices.into_iter().collect()
    }

    /// Returns all foreign-key references to the given entity.
    fn foreign_key_refs(&self, entity: &Entity) -> Vec<(&Entity,&Rel)> {
        let mut refs = vec![];
        for ent in self.entities.iter() {
            for rel in ent.rels() {
                if rel.destination == entity.name {
                    refs.push((ent,rel));
                }
            }
        }
        refs
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////
// CODEGEN


///
fn generate_entity(
    store: &Store,
    entity: &Entity,
) -> Result<TokenStream, Error>
{
    let ent = &entity.name;
    let key = entity.key_ty();
    let store_ty = store.store_type();
    let err = quote!(#CRATE::Error);
    let vis = &store.vis;
    let db_name = &store.name;
    let fields = entity.items.iter().map(|item| match item {
        AttrOrRel::Attr(Attr {
            ref name, ref ty, ..
        }) => quote!(#name: #ty),
        AttrOrRel::Rel(Rel {
            ref name,
            ref destination,
            ref multiplicity,
            ..
        }) => match multiplicity {
            ZeroOrOne => quote!(#name: Option<#destination>),
            One => quote!(#name: #destination),
            Many => quote!(#name: Vec<#destination>),
        },
    });



    // Attribute getters
    let mut attr_getters = vec![];
    for item in entity.items.iter() {
        match item {
            AttrOrRel::Attr(Attr { ref name, ref ty, ref attrs }) => {
                attr_getters.push(quote! {
                    #(#attrs)*
                    #vis fn #name <DB: ?Sized + #db_name> (self, db: &DB) -> &#ty {
                        &db.store().#ent[self].#name
                    }

                });
            }
            AttrOrRel::Rel(rel @ Rel { ref name, ref attrs, .. }) => {
                let ty = rel.foreign_key_type();
                attr_getters.push(quote!{
                    #(#attrs)*
                    #vis fn #name <DB: ?Sized + #db_name> (self, db: &DB) -> #ty {
                        db.store().#ent[self].#name
                    }
                });
            }
        }
    }

    // Attribute setters
    let mut attr_setters = vec![];
    for Attr {name, ty, ..} in entity.attrs() {
        let setter = format_ident!("set_{}", name);
        attr_setters.push(quote! {
            #vis fn #setter <DB: ?Sized + #db_name> (self, db: &mut DB, value: #ty) -> Result<(),#err> {
                db.store_mut().#ent[self].#name = value;
                Ok(())
            }
        });
    }


    // Foreign-key setters
    let mut fk_setters = vec![];
    for rel @ Rel { ref name, multiplicity, unique, .. } in entity.rels() {
        let setter = format_ident!("set_{}", name);
        let ty = rel.foreign_key_type();
        let index = rel.index_field(entity);
        let fk = &rel.name;

        let body = match (multiplicity, unique) {
            (ZeroOrOne, true) => {
                quote! {
                    if let Some(fk) = fk {
                        match self.#index.contains(fk) {
                            return Err(#err::RelationshipTooManyTargets);
                        }
                    }
                    let prev_fk = ::std::mem::replace(&mut store.#ent[self].#fk, fk);

                    if let Some(prev_fk) = prev_fk {
                        store.#index.remove(&prev_fk);
                    }
                    if let Some(fk) = fk {
                        store.#index.insert(fk, self);
                    }
                }
            }
            (ZeroOrOne, false) => {
                quote! {
                    let prev_fk = ::std::mem::replace(&mut store.#ent[self].#fk, fk);
                    if let Some(prev_fk) = prev_fk {
                        store.#index.remove(&(prev_fk, self));
                    }
                    if let Some(fk) = fk {
                        store.#index.insert((fk, self), ());
                    }
                }
            }
            (One, true) => {
                quote! {
                    match self.#index.contains(fk) {
                        return Err(#err::RelationshipTooManyTargets);
                    }
                    let prev_fk = ::std::mem::replace(&mut store.#ent[self].#fk, fk);
                    store.#index.remove(&(prev_fk, self));
                    store.#index.insert((fk, self), ());
                }
            }
            (One, false) => {
                quote! {
                    let prev_fk = ::std::mem::replace(&mut store.#ent[self].#fk, fk);
                    store.#index.remove(&(prev_fk, self));
                    store.#index.insert((fk, self), ());
                }
            }
            _ => unimplemented!(),
        };

        fk_setters.push(quote! {
            #vis fn #setter <DB: ?Sized + #db_name> (self, db: &mut DB, fk: #ty) -> Result<(),#err> {
                let mut store = db.store_mut();
                #body
                Ok(())
            }
        });
    }

    // Insert method
    let insert_method = {
        // Integrity checks before inserting a new entity
        let mut before_insert = TokenStream::new();
        // Statements after inserting a new entity (update relation indices)
        let mut update_indices = TokenStream::new();

        for rel in entity.rels() {
            let fk = &rel.name;
            let index = rel.index_field(entity);
            match rel.multiplicity {
                ZeroOrOne => {
                    // * to 0..1
                    update_indices.append_all(
                        quote! {
                        if let Some(k) = data.#fk {
                            self.#index.insert((k, next_id),());
                        }
                    }
                    );
                }
                One => {
                    // * to 1
                    update_indices.append_all(
                        quote! {
                        self.#index.insert((data.#fk, next_id),());
                    }
                    );
                }
                _ => {
                    todo!("unique constraints")
                }
            }
        }

        quote! {
            fn insert(&mut self, f: impl FnOnce(#key) -> #ent) -> Result<#key, #err> {
                let next_id = self.#ent.next_id();
                let data = f(next_id);
                #before_insert
                #update_indices
                Ok(self.#ent.insert_at(data))
            }
        }
    };

    let remove_method = {
        let mut before_remove = TokenStream::new();
        let mut update_indices = TokenStream::new();
        let mut update_foreign_keys = TokenStream::new();

        // index integrity
        for rel in entity.rels() {
            let fk = &rel.name;
            let index = rel.index_field(entity);
            match rel.multiplicity {
                ZeroOrOne => {
                    // * to 0..1
                    update_indices.append_all(
                        quote! {
                            if let Some(k) = data.#fk {
                                self.#index.remove(&(k, id));
                            }
                        }
                    );
                }
                One => {
                    // * to 1
                    update_indices.append_all(
                        quote! {
                            self.#index.remove(&(data.#fk, id));
                        }
                    );
                }
                _ => {
                    todo!("unique constraints")
                }
            }
        }

        // removal process:
        // - for each reference to the entity via foreign-keys:
        //     - if delete mode is cascade: if there are any references in the index, recursively check that the entity can be deleted
        //     - if delete mode is deny: return an error
        //     - if delete mode is nullify: OK
        // - remove the entity from the store
        // - update indices and update foreign keys
        //      - if delete mode is cascade: recursively remove all entities that reference the entity (using the index)
        //      - if delete mode is nullify: set all foreign keys to null (using the index)

        // Complications: entities can be removed via multiple cascade paths
        // -> this suddenly increases the complexity by a lot


        for (fk_ent, fk_rel) in store.foreign_key_refs(entity) {

            let index = fk_rel.index_field(fk_ent);

            for fk_rel in fk_ent.rels().filter(|r| &r.destination == ent) {
                let index = fk_rel.index_field(fk_ent);
                let fk_ent = &fk_ent.name;
                let fk = &fk_rel.name;


                // RelIndex::get_all: return all values matching the given key
                // RelIndex::remove_all: remove all values matching the given key, returns an iterator over the removed values
            }
        }

        quote! {
            fn remove(&mut self, id: #key) -> Result<#ent, #err> {

                let data = self.#ent.remove(id).ok_or(#err::EntityNotFound)?;
                #before_remove
                #update_indices
                Ok(data)
            }
        }
    };

    let res = quote! {
        #[derive(Clone)]
        #vis struct #ent {
            id: #key,
            #(#fields,)*
        }

        impl ::std::ops::Index<#key> for #store_ty {
            type Output = #ent;
            fn index(&self, key: #key) -> &Self::Output {
                &self.#ent[key]
            }
        }

        impl ::std::ops::IndexMut<#key> for #store_ty {
            fn index_mut(&mut self, key: #key) -> &mut Self::Output {
                &mut self.#ent[key]
            }
        }

        impl #CRATE::EntityStore<#ent> for #store_ty {
            #insert_method
            #remove_method

            fn remove(&mut self, s: #key) -> Result<#ent, #err> {
                todo!()
            }

            fn delta<'a>(&'a self, other: &'a Self) -> impl Iterator<Item = #CRATE::Delta<&'a #ent>> + 'a {
                self.#ent.delta(&other.#ent)
            }

            fn iter<'a>(&'a self) -> impl Iterator<Item = &'a #ent> + 'a {
                self.#ent.iter()
            }
        }

        impl #ent {
            #vis fn all <DB: ?Sized + #db_name> (db: &DB) -> impl Iterator<Item = &#ent> + '_ {
                db.store().#ent.values()
            }
        }

        #[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
        #[repr(transparent)]
        #vis struct #key(::std::num::NonZeroU32);

        impl #key {
            #vis const MIN: Self = Self(::std::num::NonZeroU32::MIN);
            #vis const MAX: Self = Self(::std::num::NonZeroU32::MAX);

            #(#attr_getters)*
            #(#attr_setters)*
            #(#fk_setters)*
        }

        impl #CRATE::EntityId for #key {
            type Entity = #ent;

            fn to_u32(self) -> u32 {
                self.0.get() - 1
            }

            fn from_u32(i: u32) -> Self {
                Self(unsafe { ::std::num::NonZeroU32::new_unchecked(i + 1) })
            }
        }

        impl #CRATE::Entity for #ent {
            type Id = #key;
            type Store = #store_ty;
            fn id(&self) -> Self::Id {
                self.id
            }
        }
    };

    Ok(res)
}

/*
/// Generates the method (a tuple `(signature, implementation)`) that sets the specified foreign key attribute.
///
/// # Example
///
/// For `rel album: Album` in `Track`:
/// ```ignore
/// fn set_track_album(&mut self, src: Track, dst: Album) -> Result<(), Error>;
/// ```
///
fn generate_set_foreign_key_method(
    store: &Store,
    src: &Entity,
    rel: &Rel,
    out_tokens: &mut TokenStream,
) -> Result<(), Error>
{
    let inv_rel = rel.inverse(store);
    let rd = &rel.name;
    use Multiplicity::*;

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
            fn #rel_name (&mut self, s: #t_src, fk: #t_dst) -> Result<(), #CRATE::Error> {

                // FK index is (FK,S) -> () for regular foreign keys
                //             FK -> S for keys with `unique` constraint

                //self.#fk_index
                let prev_fk = ::std::mem::replace(&mut self[s].#fk, fk);
                // update indices
                self.#fk_index.remove(s,prev_fk);
                self.#fk_index.insert(s,fk);

                Ok(())
            }
        }
    });

    Ok(())
}*/

/*fn generate_rel_impls(store: &Store, out_tokens: &mut TokenStream) {
    for entity in store.entities.iter() {
        for rel in entity.rels() {
            let rel_ty_name = format_ident!("Rel_{}_{}", entity.name, rel.name);
            let inv_rel_ty_name = format_ident!("Rel_{}_{}_inv", entity.name, rel.name);
            let index_name = format_ident!("index_{}_{}", entity.name, rel.name);
            let rel_src = &entity.name;
            let rel_dst = &rel.destination;
            let rel_fk = &rel.name;
            let rel_kind = match rel.multiplicity {
                One => quote!(index_N_to_1),
                _ => unimplemented!(),
            };
            out_tokens.append_all(quote! {
                #CRATE::#rel_kind!(#rel_ty_name, #rel_src, #rel_fk, #rel_dst, #inv_rel_ty_name, #index_name);
            });
        }
    }
}*/

pub(crate) fn generate_store(input: proc_macro::TokenStream) -> syn::Result<TokenStream> {
    let store: Store = syn::parse(input)?;

    // name of the wrapper trait (e.g. `MusicDb`)
    let trait_name = &store.name;
    // name of the struct that stores the data (e.g. `MusicDbStore`)
    let store_name = format_ident!("{}Store", trait_name);

    //let mut impls = TokenStream::new();


    // generate code for each entity
    let mut entities = vec![];
    for entity in store.entities.iter() {
        entities.push(generate_entity(&store, entity)?);
    }

    // Relation impls
    //generate_rel_impls(&store, &mut impls);

    // Store fields
    let mut fields = TokenStream::new();
    for entity in store.entities.iter() {
        for rel in entity.rels() {
            let index_name = rel.index_field(entity);
            let rel_src = entity.key_ty();
            let rel_dst = &rel.destination;
            let index_ty = match (rel.multiplicity, rel.unique) {
                (One | ZeroOrOne, false) => quote!(#CRATE::im::OrdMap<(#rel_dst, #rel_src),()>),
                (One | ZeroOrOne, true) => quote!(#CRATE::im::OrdMap<#rel_dst, #rel_src>),
                _ => unimplemented!(),
            };
            fields.append_all(quote! {
                #index_name: #index_ty,
            });
        }
        let name = &entity.name;
        fields.append_all(quote! {
            #name: #CRATE::Table<#name>,
        });
    }

    let vis = &store.vis;
    let attrs = &store.attrs;
    let code = quote! {
        #(#attrs)*
        #[derive(Clone, Default)]
        #[allow(non_snake_case)]
        #vis struct #store_name {
            #fields
        }

        impl #store_name {
            #vis fn new() -> #store_name {
                Self::default()
            }
        }

        #(#entities)*

        #vis trait #trait_name: #CRATE::HasStore<#store_name> {
            fn insert<E: #CRATE::Entity>(&mut self, f: impl FnOnce(E::Id) -> E) -> Result<E::Id, #CRATE::Error> where #store_name: #CRATE::EntityStore<E>;
            fn remove<E: #CRATE::Entity>(&mut self, id: E::Id) -> Result<E, #CRATE::Error> where #store_name: #CRATE::EntityStore<E>;
        }

        impl<DB: ?Sized> #trait_name for DB where DB: #CRATE::HasStore<#store_name> {
            fn insert<E: #CRATE::Entity>(&mut self, f: impl FnOnce(E::Id) -> E) -> Result<E::Id, #CRATE::Error> where #store_name: #CRATE::EntityStore<E> {
                self.store_mut().insert(f)
            }
            fn remove<E: #CRATE::Entity>(&mut self, id: E::Id) -> Result<E, #CRATE::Error> where #store_name: #CRATE::EntityStore<E> {
                self.store_mut().remove(id)
            }
        }
    };

    Ok(code)
}
