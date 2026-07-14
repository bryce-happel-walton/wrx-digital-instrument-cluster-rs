use crate::data::units::Unit;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use tokio::sync::watch;

use std::{
    fmt::{self, Debug},
    ops::RangeInclusive,
};

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, PartialOrd)]
pub struct Bound<T> {
    value: T,
    step: T,
    start: T,
    end: T,
}

impl<T> Bound<T>
where
    T: PartialOrd + PartialEq + Copy + Clone + Default + Serialize,
{
    pub fn new(init: T, range: RangeInclusive<T>, step: T) -> Self {
        Self {
            value: init,
            step,
            start: *range.start(),
            end: *range.end(),
        }
    }

    pub fn value(&self) -> T {
        self.value
    }

    pub fn set(&mut self, value: T) {
        if (self.start..=self.end).contains(&value) {
            self.value = value
        }
    }

    pub fn step(&self) -> T {
        self.step
    }

    pub fn start(&self) -> T {
        self.start
    }

    pub fn end(&self) -> T {
        self.end
    }
}

impl<T> Default for Bound<T>
where
    T: PartialOrd + PartialEq + Copy + Clone + Default + Serialize,
{
    fn default() -> Self {
        Self::new(
            Default::default(),
            T::default()..=T::default(),
            T::default(),
        )
    }
}

impl<T> From<T> for Bound<T>
where
    T: PartialOrd + PartialEq + Copy + Clone + Default + Serialize,
{
    fn from(value: T) -> Self {
        let mut ret = Self::default();
        ret.set(value);
        ret
    }
}

pub struct Parameter<T>(watch::Sender<T>);

impl<T> Parameter<T>
where
    T: Clone + Default + Serialize + PartialEq,
{
    pub fn new(value: T) -> Self {
        let (changed, _) = watch::channel(value);

        Self(changed)
    }

    pub fn value(&self) -> T {
        self.0.borrow().clone()
    }

    pub fn set_value(&self, value: T) {
        self.0.send_if_modified(|current| {
            if *current != value {
                *current = value;
                true
            } else {
                false
            }
        });
    }

    pub fn watch(&self) -> watch::Receiver<T> {
        self.0.subscribe()
    }
}

impl<T> Default for Parameter<T>
where
    T: Clone + Default + Serialize + PartialEq,
{
    fn default() -> Self {
        Self::new(Default::default())
    }
}

impl<T> Serialize for Parameter<T>
where
    T: Clone + Default + Serialize + PartialEq,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let ret = self.value().serialize(serializer);

        ret
    }
}

impl<'de, T> Deserialize<'de> for Parameter<T>
where
    T: Deserialize<'de> + Clone + Default + Serialize + PartialEq,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        T::deserialize(deserializer).map(Parameter::new)
    }
}

impl<T, U> From<U> for Parameter<T>
where
    T: From<U> + Clone + Default + Serialize + PartialEq,
    U: Clone + Default + Serialize + PartialEq,
{
    fn from(value: U) -> Self {
        Parameter::new(T::from(value))
    }
}

pub struct DataParameter<T> {
    min: T,
    max: T,

    units: Unit,

    changed: watch::Sender<T>,
}

impl<T> DataParameter<T>
where
    T: Copy + Default + PartialEq + PartialOrd + Debug,
{
    pub fn new(min: T, max: T, value: Option<T>, units: Option<Unit>) -> Self {
        let (changed, _) = watch::channel(value.unwrap_or_default());

        Self {
            min: min,
            max: max,

            units: units.unwrap_or_default(),

            changed,
        }
    }

    pub fn set_value(&self, value: T) {
        self.changed.send_if_modified(|current| {
            if *current != value {
                *current = value;
                true
            } else {
                false
            }
        });
    }

    #[allow(unused)]
    pub fn units(&self) -> Unit {
        self.units
    }

    #[allow(unused)]
    pub fn value(&self) -> T {
        *self.changed.borrow()
    }

    #[allow(unused)]
    pub fn min(&self) -> T {
        self.min
    }

    #[allow(unused)]
    pub fn max(&self) -> T {
        self.max
    }

    pub fn watch(&self) -> watch::Receiver<T> {
        self.changed.subscribe()
    }
}

impl<T> Default for DataParameter<T>
where
    T: Copy + Default + PartialEq + PartialOrd + Debug,
{
    fn default() -> Self {
        Self::new(Default::default(), Default::default(), None, None)
    }
}

impl<T> From<T> for DataParameter<T>
where
    T: Copy + Default + PartialEq + PartialOrd + Debug,
{
    fn from(value: T) -> Self {
        Self::new(Default::default(), Default::default(), Some(value), None)
    }
}

/// Used within other macros to allow for *optional* default values by way of an `expr` token
///
/// Note: `bool` defaults to `true` unless otherwise stated
#[macro_export]
#[doc(hidden)]
macro_rules! __default_value {
    ($type:path| $param_default:expr) => {
        $param_default.into()
    };
    (bool|) => {
        true.into()
    };
    (String|) => {
        String::from("Unknown").into()
    };
    ($type:path|) => {
        Default::default()
    };
}

/// Executes a match on the value of the specified parameter.
/// Re-executes upon parameter updates.
#[macro_export]
macro_rules! parameter_match {
    ($param:expr => { $($val:pat => $blk:block),* $(,)? }) => {{
        let __param = $param;
        tokio::spawn(async move {
            let mut watch = $param.watch();
            let mut value = $param.value();

            loop {
                 match value {
                    $($val => $blk),*
                    _ => {}
                }

                tokio::select! {
                    Ok(_) = watch.changed() => {
                        value = *watch.borrow_and_update();
                    },
                    else => {break;}
                }
            }
        });
    }}
}

/// The visibility and permissions of nodes is only relevant for user-facing frontend
#[allow(unused)]
#[derive(Debug)]
pub enum Node {
    /// inherently read-only
    HiddenParameter,
    ReadOnlyParameter {
        name: &'static str,
        ty: &'static str,
    },
    Parameter {
        name: &'static str,
        ty: &'static str,
    },
    Page {
        name: &'static str,
        items: Box<[&'static Node]>,
    },
}

impl Node {
    fn fmt_with_indent(&self, f: &mut fmt::Formatter<'_>, indent: usize) -> fmt::Result {
        let pad = "  ".repeat(indent);

        match self {
            Node::HiddenParameter => {
                writeln!(f, "{pad} <hidden item>")
            }
            Node::ReadOnlyParameter { name, ty } => {
                writeln!(f, "{pad}[RO] {name}: {ty}")
            }
            Node::Parameter { name, ty } => {
                writeln!(f, "{pad}[RW] {name}: {ty}")
            }
            Node::Page { name, items } => {
                writeln!(f, "{pad}{name} {{")?;
                for item in items.iter() {
                    item.fmt_with_indent(f, indent + 1)?;
                }
                writeln!(f, "{pad}}}")
            }
        }
    }
}

impl fmt::Display for Node {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.fmt_with_indent(f, 0)
    }
}

pub trait ParameterStruct {
    fn apply(&self, from: Self)
    where
        Self: Sized;

    fn set_by_path(&self, param_path: &str, value: &str);

    fn get_by_path(&self, param_path: &str) -> Option<(String, Box<dyn std::any::Any>)>;

    fn get_page_layout(&self) -> &'static Node;

    fn unroll_path<'a>(
        &self,
        split_path: &Vec<&str>,
        current_path: String,
        current_depth: usize,
        node: Option<&'a Node>,
    ) -> (&'a Node, String) {
        let node = node.unwrap_or(self.get_page_layout());
        if split_path.len() != current_depth {
            if let Node::Page { name: _, items } = node {
                for item in items {
                    if let Node::Page { name, items: _ } = item {
                        if let Some(current_item) = split_path.get(current_depth + 1) {
                            if current_item == name {
                                return self.unroll_path(
                                    split_path,
                                    format!("{}.", split_path[..=current_depth].join(".")),
                                    current_depth + 1,
                                    Some(item),
                                );
                            }
                        }
                    }
                }
            }
        }

        (node, current_path)
    }

    fn resolve_node<'a>(&self, path: &'a str, layout_node: Option<&'a Node>) -> Option<&'a str> {
        if let Node::Page { name, items: _ } = layout_node.unwrap_or(self.get_page_layout()) {
            path.strip_prefix(&format!("{}.", *name))
        } else {
            None
        }
    }

    fn resolve_path(&self, path: &str) -> Option<(String, Box<dyn std::any::Any>)> {
        self.resolve_node(path, None)
            .and_then(|path| self.get_by_path(path))
    }
}

/// Defines a serdes capable structure with parameter fields and optional default values.
/// Capable of having multiple pages
/// TODO: add example (src/application/settings.rs:@Settings)
#[macro_export]
macro_rules! parameter_struct {
    ($([$condition:expr])? $page:ident { $($items:tt)* }) => {
        $crate::parameter_struct!(@page $([$condition])? $page
            { }
            { }
            { }
            { }
            $($items)*
        );
    };

    (@page $([$condition:expr])? $page:ident
        { $($params:tt)* }
        { $($inits:tt)* }
        { $($defs:tt)* }
        { $($entries:tt)* }
    ) => {pastey::paste!{
        $($defs)*

        #[derive(serde::Serialize, serde::Deserialize)]
        #[allow(non_camel_case_types)]
        pub struct [<$page:camel_edge>] {
            $($params)*
        }

        #[allow(unused)]
        impl $crate::data::parameters::ParameterStruct for [<$page:camel_edge>] {
            fn apply(&self, from: Self) {
                $crate::parameter_struct!(@apply self, from| $($entries)*);
            }

            fn set_by_path(&self, param_path: &str, value: &str) {
                let (target, path) = match param_path.split_once('.') {
                    Some((root, path)) => {(root, path)}
                    _ => {(param_path, "")}
                };

                $crate::parameter_struct!(@path-set self target; path; value| $($entries)*)
            }

            fn get_by_path(&self, param_path: &str) -> Option<(String, Box<dyn std::any::Any>)> {
                let (target, path) = match param_path.split_once('.') {
                    Some((root, path)) => {(root, path)}
                    _ => {(param_path, "")}
                };

                $crate::parameter_struct!(@path-get self target; path;| $($entries)*)
            }

            fn get_page_layout(&self) -> &'static $crate::data::parameters::Node {
                static [<$page:upper _LAYOUT>]: std::sync::OnceLock<$crate::data::parameters::Node> = std::sync::OnceLock::new();
                [<$page:upper _LAYOUT>].get_or_init(||
                    $crate::parameter_struct!(@node $page self| $($entries)*)
                )
            }
        }

        impl Default for [<$page:camel_edge>] {
            fn default() -> Self {
                Self {
                    $($inits)*
                }
            }
        }
    }};

    (@page $page:ident
        { $($params:tt)* }
        { $($inits:tt)* }
        { $($defs:tt)* }
        { $($entries:tt)* }
        $([$permissions:tt])? $param:ident: $ty:path $(= $val:expr)?, $($rest:tt)*
    ) => {
        $crate::parameter_struct!(@page $page
            { $($params)* pub $param: $crate::data::parameters::Parameter<$ty>, }
            { $($inits)* $param: $crate::__default_value!($ty| $($val)?), }
            { $($defs)* }
            { $($entries)* (param $([$permissions])? $param: $ty) }
            $($rest)*
        );
    };

    (@page $([$condition:expr])? $page:ident
        { $($params:tt)* }
        { $($inits:tt)* }
        { $($defs:tt)* }
        { $($entries:tt)* }
        $([$sub_condition:expr])? $sub:ident { $($inner:tt)* }, $($rest:tt)*
    ) => {pastey::paste!{
        $crate::parameter_struct!(@page $page
            { $($params)* pub $sub: [<$sub:camel_edge>], }
            { $($inits)* $sub: Default::default(), }
            { $($defs)* $crate::parameter_struct!($([$condition])? $sub { $($inner)* }); }
            { $($entries)* (page $([$sub_condition])? $sub) }
            $($rest)*
        );
    }};

    (@apply $self:ident, $from:ident|) => {};
    (@apply $self:ident, $from:ident| (param $([$permissions:tt])? $param:ident: $ty:path) $($rest:tt)*) => {
        $self.$param.set_value($from.$param.value());
        $crate::parameter_struct!(@apply $self, $from| $($rest)*)
    };
    (@apply $self:ident, $from:ident| (page $([$condition:expr])? $sub:ident) $($rest:tt)*) => {
        $self.$sub.apply($from.$sub);
        $crate::parameter_struct!(@apply $self, $from| $($rest)*)
    };

    (@node-internal $self:ident| param [hidden] $param:ident: $ty:path) => {
        &$crate::data::parameters::Node::HiddenParameter
    };
    (@node-internal $self:ident| param [ro] $param:ident: $ty:path) => {
        &$crate::data::parameters::Node::ReadOnlyParameter{
            name: stringify!($param),
            ty: stringify!($ty)
        }
    };
    (@node-internal $self:ident| param $param:ident: $ty:path) => {
        &$crate::data::parameters::Node::Parameter{
            name: stringify!($param),
            ty: stringify!($ty)
        }
    };
    (@node-internal $self:ident| page [hidden] $sub:ident) => {
        &$crate::data::parameters::Node::HiddenParameter
    };
    (@node-internal $self:ident| page [$sub_condition:expr] $sub:ident) => {
        if $sub_condition {
            $self.$sub.get_page_layout()
        } else {
            $crate::parameter_struct!(@node-internal $self| page [hidden] $sub)
        }
    };
    (@node-internal $self:ident| page $sub:ident) => {
        $self.$sub.get_page_layout()
    };
    (@node $page:ident $self:ident| $(($([$condition:expr])? $node:tt $([$permissions:tt])? $thing:ident $(: $ty:path)?))*) => {
        $crate::data::parameters::Node::Page{
            name: stringify!($page),
            items: Box::new([$(
                $crate::parameter_struct!(@node-internal $self| $node $([$condition])? $([$permissions])? $thing $(: $ty)?)
            ),*])
        }
    };

    (@path-set-internal $self:ident $path:expr; $value:ident| param [hidden] $param:ident: $ty:path) => {
        eprintln!("Failed to set {} to {:?}: Parameter is hidden; inherently read-only", stringify!($param), $value)
    };
    (@path-set-internal $self:ident $path:expr; $value:ident| param [ro] $param:ident: $ty:path) => {
        eprintln!("Failed to set {} to {:?}: Parameter is read-only", stringify!($param), $value)
    };
    (@path-set-internal $self:ident $path:expr; $value:ident| param $param:ident: $ty:path) => {
        match serde_json::from_str::<$ty>($value) {
            Ok(value) => $self.$param.set_value(value),
            Err(e) => eprintln!("Failed to set {} to {:?}: {e:?}", stringify!($param), $value)
        }
    };
    (@path-set-internal $self:ident $path:expr; $value:ident| page $([$condition:expr])? $sub:ident) => {
        $self.$sub.set_by_path($path, $value);
    };
    (@path-set $self:ident $target:expr; $path:expr; $val:ident| $(($node:tt $([$permissions:tt])? $thing: ident $(: $ty:path)?))*) => {
        match $target {
            $(stringify!($thing) => {
                $crate::parameter_struct!(@path-set-internal $self $path; $val| $node $([$permissions])? $thing $(: $ty)?);
            }),*
            _ => {
                println!("Path [{}] does not exist!", $path);
            }
        }
    };

    (@path-get-internal $self:ident $path:expr;| param $([$permissions:tt])? $param:ident: $ty:path) => {{
        let value = $self.$param.value();
        Some((serde_json::to_string(&value).unwrap(), Box::new(value)))
    }};
    (@path-get-internal $self:ident $path:expr;| page $([$condition:expr])? $sub:ident) => {
        $self.$sub.get_by_path($path)
    };
    (@path-get $self:ident $target:expr; $path:expr;| $(($node:tt $([$permissions:tt])? $thing: ident $(: $ty:path)?))*) => {
        match $target {
            $(stringify!($thing) => {
                $crate::parameter_struct!(@path-get-internal $self $path;| $node $([$permissions])? $thing $(: $ty)?)
            }),*
            _ => {
                println!("Path [{}] does not exist!", $path);
                None
            }
        }
    };
}
