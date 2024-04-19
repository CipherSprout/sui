//# init --edition development

//# publish
module 0x42::m {

    public enum Option<T> has drop {
        None,
        Some(T)
    }

    fun foo(): bool {
        false
    }

    public fun t0(): u64 {
        let o = &mut Option::Some(0);
        match (o) {
            Option::None => 0,
            _ if ({*o = Option::None; false}) => 1,
            Option::Some(_) => 2,
        }
    }

    public fun t1(): u64 {
        let o = &mut Option::Some(0);
        match (o) {
            Option::None => 0,
            _ if ({*o = Option::None; true }) => 1,
            Option::Some(_) => 2,
        }
    }
}

//# run
module 0x42::main {
    use 0x42::m;
    fun main() {
        assert!(m::t0() == 2);
        assert!(m::t1() == 1);
    }
}
