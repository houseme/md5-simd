/// Unoptimized wide-SIMD kernels can exceed the default test-thread stack.
pub fn run_with_large_stack(test: fn()) {
    std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(test)
        .expect("spawn large-stack SIMD test")
        .join()
        .expect("large-stack SIMD test");
}
