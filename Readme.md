# Build Exact

This was an experiment I did to test the feasibility of making a simple build system in the style of Bazel - with precise dependency tracking.

This version uses Deno to describe the build graph and Rust to build it. I have a separate project (see the `sandbox` directory) to provide filesystem sandboxing so dependency links can't be missed.

Deno turned out to be a poor choice. One of the problems with build systems is that in general you can't know the full build graph (DAG) before you start building. For example if you generate come C++ code you're probably going to need to scan that code to see which headers it uses. Some of those might be generated too!

So you always need to be able to run a bit of build system code during the build. Given that it makes way more sense to choose a language that can be properly sandboxed itself. I have some half written code to switch to Starlark, which seems to be the most reasonable option at the moment (Bazel uses it). But I have abandoned this project.

I only abandoned it because of internal company politics. The ideas are good - you should use something like Bazel, Buck or Pants 2 if you are building anything remotely big.
