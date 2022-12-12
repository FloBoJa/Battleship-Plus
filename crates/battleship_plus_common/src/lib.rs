pub mod messages {
    include!(concat!(env!("OUT_DIR"), "/battleshipplus.rs"));
    include!(concat!(env!("OUT_DIR"), "/battleshipplus_op_codes.rs"));
}