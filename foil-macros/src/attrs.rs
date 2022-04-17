use std::collections::HashMap;
use syn::{spanned::Spanned, Attribute, Error, Ident, Lit, Meta, MetaList, NestedMeta, Result};

pub struct Attrs(HashMap<Ident, Meta>);

impl Attrs {
    pub fn extract(input: Vec<Attribute>) -> Result<Self> {
        let mut attrs = HashMap::new();
        for attr in input.into_iter().filter(|attr| attr.path.is_ident("foil")) {
            let meta = attr.parse_meta()?;
            if let Meta::List(meta_list) = meta {
                collect_meta_list_to_map(meta_list, &mut attrs)?;
            } else {
                return Err(Error::new(meta.span(), "expected `MetaList`"));
            }
        }

        Ok(Self(attrs))
    }

    pub fn keys(&self) -> impl Iterator<Item = &Ident> {
        self.0.keys()
    }

    pub fn get_path<P: ?Sized>(&mut self, path: &P) -> Result<bool>
    where
        Ident: PartialEq<P>,
    {
        if let Some(meta) = self.remove(path) {
            if let Meta::Path(_) = meta {
                Ok(true)
            } else {
                Err(Error::new(meta.span(), "expected `MetaList`"))
            }
        } else {
            Ok(false)
        }
    }

    pub fn get_list<P: ?Sized>(&mut self, path: &P) -> Result<Option<Attrs>>
    where
        Ident: PartialEq<P>,
    {
        let mut attrs = HashMap::new();
        if let Some(meta) = self.remove(path) {
            if let Meta::List(meta_list) = meta {
                collect_meta_list_to_map(meta_list, &mut attrs)?;
                Ok(Some(Self(attrs)))
            } else {
                Err(Error::new(meta.span(), "expected `MetaList`"))
            }
        } else {
            Ok(None)
        }
    }

    pub fn get_name_value<P: ?Sized>(&mut self, path: &P) -> Result<Option<Lit>>
    where
        Ident: PartialEq<P>,
    {
        if let Some(meta) = self.remove(path) {
            if let Meta::NameValue(meta_name_value) = meta {
                Ok(Some(meta_name_value.lit))
            } else {
                Err(Error::new(meta.span(), "expected `MetaNameValue`"))
            }
        } else {
            Ok(None)
        }
    }

    pub fn ignore<P: ?Sized>(&mut self, paths: &[&P])
    where
        Ident: PartialEq<P>,
    {
        for path in paths {
            self.remove(path);
        }
    }

    pub fn done(&self) -> Result<()> {
        if let Some((_, meta)) = self.0.iter().next() {
            Err(Error::new(meta.span(), "unexpected attribute"))
        } else {
            Ok(())
        }
    }

    fn remove<P: ?Sized>(&mut self, path: &P) -> Option<Meta>
    where
        Ident: PartialEq<P>,
    {
        if let Some(key) = self.0.keys().find(|key| *key == path).cloned() {
            Some(self.0.remove(&key).unwrap())
        } else {
            None
        }
    }
}

fn collect_meta_list_to_map(meta_list: MetaList, map: &mut HashMap<Ident, Meta>) -> Result<()> {
    for nested in meta_list.nested {
        if let NestedMeta::Meta(meta) = nested {
            let path = meta.path();
            let ident = meta
                .path()
                .get_ident()
                .ok_or_else(|| Error::new(path.span(), "expected `Ident`"))?
                .clone();
            map.insert(ident, meta);
        } else {
            return Err(Error::new(nested.span(), "expected `Meta`"));
        }
    }

    Ok(())
}
