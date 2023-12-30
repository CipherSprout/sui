module 0x42::m {

    fun t0(): u64 {
        return 5
    }
    
    fun t1(cond: bool): u64 {
        if (cond) { return 5 } else { abort 0 }
    }
    
    fun t2(cond: bool): u64 {
        if (cond) { return 5 } else { return 0 }
    }
}
