// This is a Deno script to generate a JSON file

import { exportBuild } from "../buildinfo/std.ts";

// Directory of this script, with trailing slash.
const thisDir = new URL('.', import.meta.url).pathname;

function rel(x: string[]): string[] {
  return x.map(p => thisDir + p);
}

exportBuild({
  commands: [
    {
      command: ["mkdir", "build"],
      inputs: [],
      outputs: rel([
        "build",
      ]),
      workingDir: thisDir,
      env: {},
    },
    {
      command: ["clang++", "-c", "foo.cpp", "-o", "build/foo.o"],
      inputs: rel([
        "build",
        "foo.cpp",
        "foo.h",
      ]),
      outputs: rel([
        "build/foo.o",
      ]),
      workingDir: thisDir,
      env: {},
    },
    {
      command: ["clang++", "-c", "bar.cpp", "-o", "build/bar.o"],
      inputs: rel([
        "build",
        "bar.cpp",
        "bar.h",
        "foo.h",
      ]),
      outputs: rel([
        "build/bar.o",
      ]),
      workingDir: thisDir,
      env: {},
    },
    {
      command: ["clang++", "-c", "main.cpp", "-o", "build/main.o"],
      inputs: rel([
        "build",
        "main.cpp",
        "bar.h",
        "foo.h",
      ]),
      outputs: rel([
        "build/main.o",
      ]),
      workingDir: thisDir,
      env: {},
    },
    {
      command: ["clang++", "build/foo.o", "build/bar.o", "build/main.o", "-o", "build/main"],
      inputs: rel([
        "build",
        "build/foo.o",
        "build/bar.o",
        "build/main.o",
      ]),
      outputs: rel([
        "build/main",
      ]),
      workingDir: thisDir,
      env: {},
    },
    {
      command: ["clang++", "-c", "foo_test.cpp", "-o", "build/foo_test.o"],
      inputs: rel([
        "build",
        "foo_test.cpp",
        "foo.h",
      ]),
      outputs: rel([
        "build/foo_test.o",
      ]),
      workingDir: thisDir,
      env: {},
    },
    {
      command: ["clang++", "build/foo.o", "build/foo_test.o", "-o", "build/foo_test"],
      inputs: rel([
        "build",
        "build/foo.o",
        "build/foo_test.o",
      ]),
      outputs: rel([
        "build/foo_test",
      ]),
      workingDir: thisDir,
      env: {},
    },
  ],
  tests: {
    "main": {
      command: ["./main"],
      inputs: rel([
        "build/main",
      ]),
      workingDir: thisDir + "/build",
      env: {},
    },
    "foo": {
      command: ["./foo_test"],
      inputs: rel([
        "build/foo_test",
      ]),
      workingDir: thisDir + "/build",
      env: {},
    },
  },
  sandboxedDirs: [
    thisDir,
  ],
});
