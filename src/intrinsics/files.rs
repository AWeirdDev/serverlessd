// the intrinsics files (intrinsics/) will be built into the binary
// which is rather more logical
// we don't want the user installing shit on their own
include!(concat!(env!("OUT_DIR"), "/files.rs"));

#[inline]
pub(super) fn get_intrinsics_file(name: &str) -> Option<&str> {
    FILES
        .iter()
        .find(|(filename, _)| *filename == name)
        .map(|item| item.1)
}
