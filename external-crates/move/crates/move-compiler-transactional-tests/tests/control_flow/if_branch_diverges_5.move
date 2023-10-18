//# run
module 0x42::m {
#[allow(unneeded_return)]
fun main() {
    if (true) {
        loop break
    } else {
        assert!(false, 42);
        return ()
    }
}
}
