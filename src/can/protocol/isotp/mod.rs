mod isotp;
pub use isotp::*;

pub mod parser;

/// Implements a subsystem's `NAME` and `SERVICES` consts along with its
/// [`ISOTPSubsystem`] impl, dispatching to the service enum's
/// [`ISOTPService::demux`]. The service enum must be `#[repr(u8)]` and
/// derive `FromRepr` and `VariantArray`.
macro_rules! isotp_subsystem {
    ($subsystem:tt, $service:ty, $name:literal) => {
        pub struct $subsystem;

        impl $subsystem {
            #[allow(unused)]
            pub const NAME: &'static str = $name;

            pub const SERVICES: &'static [u8] = &{
                let mut bytes = [0u8; <$service as ::strum::VariantArray>::VARIANTS.len()];
                let mut i = 0;
                while i < bytes.len() {
                    bytes[i] = <$service as ::strum::VariantArray>::VARIANTS[i] as u8;
                    i += 1;
                }
                bytes
            };
        }

        impl $crate::can::protocol::isotp::ISOTPSubsystem for $subsystem {
            fn name(&self) -> &'static str {
                Self::NAME
            }

            fn demux(
                &self,
                service: u8,
                context: &$crate::can::protocol::isotp::DemuxContext,
            ) -> bool {
                if let Some(service) = <$service>::from_repr(service) {
                    $crate::can::protocol::isotp::ISOTPService::demux(&service, context);
                    true
                } else {
                    false
                }
            }

            fn services(&self) -> Vec<u8> {
                Self::SERVICES.to_vec()
            }
        }
    };
}

/// Declares the `SUBSYSTEMS` registry along with a compile-time check that
/// no two subsystems claim the same service byte; an overlap would demux
/// messages with the wrong parser.
macro_rules! subsystems {
    ($($subsystem:path),+ $(,)?) => {
        static SUBSYSTEMS: &[&dyn ISOTPSubsystem] = &[$(&$subsystem),+];

        const _: () = {
            const SERVICE_TABLES: &[&[u8]] = &[$(<$subsystem>::SERVICES),+];

            let mut table = 0;
            while table < SERVICE_TABLES.len() {
                let mut index = 0;
                while index < SERVICE_TABLES[table].len() {
                    let service = SERVICE_TABLES[table][index];

                    let mut other_table = table;
                    while other_table < SERVICE_TABLES.len() {
                        let mut other_index =
                            if other_table == table { index + 1 } else { 0 };

                        while other_index < SERVICE_TABLES[other_table].len() {
                            assert!(
                                SERVICE_TABLES[other_table][other_index] != service,
                                "duplicate service byte across ISOTP subsystems"
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

pub(crate) use isotp_subsystem;
pub(crate) use subsystems;
