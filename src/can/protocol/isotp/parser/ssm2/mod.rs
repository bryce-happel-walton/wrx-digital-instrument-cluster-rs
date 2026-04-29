mod ssm2;
pub use ssm2::*;

pub mod cobb;

/// Declares an SSM2 parameter enum with each variant's address sequence,
/// along with a compile-time check that every address is a whole number of
/// [`SSM2Service::ADDRESS_CHUNK_SIZE`] chunks. Register the enum with
/// [`ssm2_parameter_sets!`] to reject overlapping addresses.
macro_rules! ssm2_parameters {
    ($enum:ident {$($parameter:ident = [$($byte:literal),+ $(,)?]),+ $(,)?}) => {
        #[derive(Debug, PartialEq, Eq, ::strum::EnumIter)]
        pub enum $enum {
            $($parameter,)+
        }

        impl $enum {
            pub const ADDRESSES: &'static [&'static [u8]] =
                &[$(Self::$parameter.address()),+];

            pub const fn address(&self) -> &'static [u8] {
                match self {
                    $(Self::$parameter => &[$($byte),+],)+
                }
            }

            pub const fn data_len(&self) -> usize {
                self.address().len()
                    / $crate::can::protocol::isotp::parser::ssm2::SSM2Service::ADDRESS_CHUNK_SIZE
            }
        }

        impl TryFrom<&[u8]> for $enum {
            type Error = ();

            fn try_from(address: &[u8]) -> Result<Self, Self::Error> {
                <Self as ::strum::IntoEnumIterator>::iter()
                    .find(|parameter| parameter.address() == address)
                    .ok_or(())
            }
        }

        const _: () = {
            let mut index = 0;
            while index < <$enum>::ADDRESSES.len() {
                assert!(
                    <$enum>::ADDRESSES[index].len()
                        % $crate::can::protocol::isotp::parser::ssm2::SSM2Service::ADDRESS_CHUNK_SIZE
                        == 0,
                    "SSM2 parameter address is not a whole number of address chunks"
                );
                index += 1;
            }
        };
    };
}

pub(crate) use ssm2_parameters;

/// Declares a compile-time check that no two parameters within or across the
/// listed [`ssm2_parameters!`] enums share an address sequence; a duplicate
/// would make demuxing ambiguous. Prefix overlaps are fine — demuxing
/// resolves those by longest match.
macro_rules! ssm2_parameter_sets {
    ($($enum:path),+ $(,)?) => {
        const _: () = {
            const fn equal(a: &[u8], b: &[u8]) -> bool {
                if a.len() == b.len() {
                    let mut i = 0;
                    while i < a.len() && a[i] == b[i] {
                        i += 1;
                    }
                    i == a.len()
                } else {
                    false
                }
            }

            const ADDRESS_TABLES: &[&[&[u8]]] = &[$(<$enum>::ADDRESSES),+];

            let mut table = 0;
            while table < ADDRESS_TABLES.len() {
                let mut index = 0;
                while index < ADDRESS_TABLES[table].len() {
                    let address = ADDRESS_TABLES[table][index];

                    let mut other_table = table;
                    while other_table < ADDRESS_TABLES.len() {
                        let mut other_index =
                            if other_table == table { index + 1 } else { 0 };

                        while other_index < ADDRESS_TABLES[other_table].len() {
                            assert!(
                                !equal(ADDRESS_TABLES[other_table][other_index], address),
                                "duplicate address across SSM2 parameter enums"
                            );
                            other_index += 1;
                        }
                        other_table += 1;
                    }
                    index += 1;
                }
                table += 1;
            }
        };
    };
}

pub(crate) use ssm2_parameter_sets;
