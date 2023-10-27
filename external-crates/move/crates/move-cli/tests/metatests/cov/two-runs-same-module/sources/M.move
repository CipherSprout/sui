address 0x42 {
module M {
    #[allow(unneeded_return)]
    public entry fun test(x: u8) {
        if (x == 0) {
            return ()
        } else {
            return ()
        }
    }
}
}
