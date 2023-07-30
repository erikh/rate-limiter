# rate limiting example with davisjr

[davisjr](https://github.com/erikh/davisjr) is derived from the
[ratpack](https://github.com/zerotier/ratpack) crate I developed for zerotier
for the construction of the [coyote](https://github.com/zerotier/coyote)
project amongst other things. It was designed while grousing about the
verbosity of other rust HTTP frameworks, and is designed to be very simple
as a result. The idea is that any handler function can also be middleware,
and any handler that returns a "None" response is expected to have its
request handled by the next handler in the chain, or a 500 is returned. I
encourage you to read the docs, there should be ample.

This allowed me to keep this rate limiter very simple. It uses a map of keys
for the API keys, pointing at a map of routes, which then point at vector of
Instant time values. This could probably be cheaper as far as memory use
goes, but allowed me to keep simple pruning routines. The result is a handler
which is only a few lines of code and repurposed for any occasion.

The fed data structure is just route -> tuple of duration + count, the latter
half meaning "how many requests in how much time". The tests fix this all at 1
second for ease of testing, but try to exercise all cases. They use the testing
framework in davisjr which allows you to do HTTP request testing without
involving any syscalls.

There is also an example program that is more or less the same as the tests but
takes a YAML configuration file and turns that into the fed data structure (the
"Limit Map"). Ease of deserialization was aided by my
[fancy-duration](https://github.com/erikh/fancy-duration) crate which is very
similar to golang durations and their text representations. The provided
configuration file uses one second intervals, which are represented as "1s",
for example.

This took approximately 3 hours to code from `cargo new` to now, but I took
several breaks to think about it, and the challenge I received yesterday, but
spent most of the evening on a Friday night considering ways to solve it while
flooding my brain with malt beverages.

On Sunday (7/30), I reworked the example to have a dynamic router, so all the routes
you program into the config work, an oversight I made. I also took a clippy
pass Saturday sometime. I got bored and decided to noodle on it a bit. Hope
that's ok.

Thanks for the opportunity.

-Erik
