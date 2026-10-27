# Propia

The intent of this project is to create both a robust and reliable IPP print
server and a suite of tools and libraries that make life with IPP easier.

This project is intentionally IPP-only because we now stand at a point in
printing history where the tide has shifted toward standardization. The stars
have aligned such that the opportunity now exists for us to drop the legacy
printer stack with its drivers and lock-in and switch to a fully driverless
setup. For that to work we need more fluent tools, stronger infrastructure, and
better visibility.

I also am frustrated by the printing industry's current lack of rigor. The open
source world is so far ahead of corporate printing products in their ease of use
and reliability, and I believe many aspects and values can be carried over and
supported by open-sourcing such a product. I want a print server that can track
users and costs, operate virtual queues, load-balance between machines, and
in general provide an overwhelmingly positive experience to administrators and
sysadmins which are currently using products like PaperCut. I believe it can be
done, and I also believe Rust is the best language to do it in.
