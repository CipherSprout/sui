//# run
module 0x42::m {
#[allow(unneeded_return)]
fun main() {
    if (true) {
        if (true) return () else return ()
    } else {
        assert!(false, 42);
        return ()
    }
}
}
