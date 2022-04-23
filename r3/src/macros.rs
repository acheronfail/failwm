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
                $(#[$fmeta:meta])* $fvis:vis $field:ident => $name:expr $(; only_if_exists = $only_if_exists:expr)?,
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
                $(
                    #[allow(unused_assignments)]
                    let mut only_if_exists = true;
                    $( only_if_exists = $only_if_exists; )?
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
