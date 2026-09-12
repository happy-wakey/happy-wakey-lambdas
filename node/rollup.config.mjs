export default {
  input: "handler.mjs",
  output: {
    file: "dist/handler.bundle.mjs",
    format: "es",
    sourcemap: false,
    inlineDynamicImports: true
  },
  treeshake: true,
  external: []
};
