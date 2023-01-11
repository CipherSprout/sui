import { defineConfig } from "vite";
import dts from "vite-plugin-dts";

export default defineConfig({
  plugins: [dts()],
  build: {
    lib: {
			entry: './src/index.tsx',
      fileName: "index",
      formats: ["es", "cjs"],
    },
  },
});
