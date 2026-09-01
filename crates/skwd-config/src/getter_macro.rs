#[macro_export]
macro_rules! getters {
    () => {};
    ($fn:ident: text_setting($setting:expr); $($rest:tt)*) => {
        pub fn $fn(&self) -> String {
            $setting.read(self.root())
        }
        $crate::getters! { $($rest)* }
    };
    ($fn:ident: bool_setting($setting:expr); $($rest:tt)*) => {
        pub fn $fn(&self) -> bool {
            $setting.read(self.root())
        }
        $crate::getters! { $($rest)* }
    };
    ($fn:ident: number_setting($setting:expr); $($rest:tt)*) => {
        pub fn $fn(&self) -> f64 {
            $setting.read(self.root())
        }
        $crate::getters! { $($rest)* }
    };
    ($fn:ident: str($path:expr, $def:literal); $($rest:tt)*) => {
        pub fn $fn(&self) -> String {
            let default = $crate::schema::text_default($path).unwrap_or($def);
            $crate::str_at(self.root(), $path, default)
        }
        $crate::getters! { $($rest)* }
    };
    ($fn:ident: on_unless_off($path:expr); $($rest:tt)*) => {
        pub fn $fn(&self) -> bool {
            let default = $crate::schema::boolean_default($path).unwrap_or(true);
            $crate::bool_at(self.root(), $path, default)
        }
        $crate::getters! { $($rest)* }
    };
    ($fn:ident: off_unless_on($path:expr); $($rest:tt)*) => {
        pub fn $fn(&self) -> bool {
            let default = $crate::schema::boolean_default($path).unwrap_or(false);
            $crate::bool_at(self.root(), $path, default)
        }
        $crate::getters! { $($rest)* }
    };
    ($fn:ident: bool($path:expr, $def:literal); $($rest:tt)*) => {
        pub fn $fn(&self) -> bool {
            let default = $crate::schema::boolean_default($path).unwrap_or($def);
            $crate::bool_at(self.root(), $path, default)
        }
        $crate::getters! { $($rest)* }
    };
    ($fn:ident: num($path:expr, $def:expr); $($rest:tt)*) => {
        pub fn $fn(&self) -> f64 {
            $crate::schema::read_number(self.root(), $path)
                .unwrap_or_else(|| $crate::num_at(self.root(), $path, $def))
        }
        $crate::getters! { $($rest)* }
    };
    ($fn:ident: num($path:expr, $def:expr) as $t:ty; $($rest:tt)*) => {
        pub fn $fn(&self) -> $t {
            $crate::schema::read_number(self.root(), $path)
                .unwrap_or_else(|| $crate::num_at(self.root(), $path, $def)) as $t
        }
        $crate::getters! { $($rest)* }
    };
    ($fn:ident: sel_num($path:expr, $large:expr, $small:expr) as $t:ty; $($rest:tt)*) => {
        pub fn $fn(&self) -> $t {
            self.sel_num($path, $large, $small) as $t
        }
        $crate::getters! { $($rest)* }
    };
    ($fn:ident: self_bool_false_unless_true($path:expr); $($rest:tt)*) => {
        pub fn $fn(&self) -> bool {
            self.bool_false_unless_true($path)
        }
        $crate::getters! { $($rest)* }
    };
}
