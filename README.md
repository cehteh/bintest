Testing the executables build by a bin crate.


# Description


**`cargo test`** has no support for running tests on a build excecutable.
This crate works around this deficiency.


# How It Works

There are some problems to overcome the cargo limitations.

1. Running cargo tests does not depend on the executables to be build, by default they are not
   compiled at test time.
2. There are no standard facilities to locate and execute them in a test.

BinTest solve these problems by running **`cargo build`** at test time, parsing its output for
identifying and locating the build executables. On request it creates a std::process::Command
for the binary which can be used for any further testing.

