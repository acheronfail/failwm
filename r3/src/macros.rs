#[macro_export]
macro_rules! ret_ok_if_none {
    ($option:expr) => {{
        match $option {
            Some(x) => x,
            None => return Ok(()),
        }
    }};
}

// HACK: temporary workaround for https://github.com/rust-x-bindings/rust-xcb/pull/182
#[macro_export]
macro_rules! atoms_struct {
    (
        $(#[$outer:meta])*
        $vis:vis struct $Atoms:ident {
            $(
                $(#[$fmeta:meta])* $fvis:vis $field:ident => $name:expr,
            )*
        }
    ) => {
        $(#[$outer])*
        $vis struct $Atoms {
            $($(#[$fmeta])* $fvis $field: xcb::x::Atom,)*
        }
        impl $Atoms {
            #[allow(dead_code)]
            pub fn intern_all(conn: &xcb::Connection) -> xcb::Result<$Atoms> {
                $Atoms::intern_all_with_exists(conn, true)
            }

            pub fn intern_all_with_exists(conn: &xcb::Connection, only_if_exists: bool) -> xcb::Result<$Atoms> {
                $(
                    let $field = conn.send_request(&xcb::x::InternAtom {
                        only_if_exists,
                        name: $name,
                    });
                )*
                $(
                    let $field = conn.wait_for_reply($field)?.atom();
                )*
                Ok($Atoms {
                    $($field,)*
                })
            }
        }
    };
}
