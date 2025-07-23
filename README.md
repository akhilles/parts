# parts

This project is a PCB part library, that is meant to provide a semi-curated + structured set of parts for use in PCB design. It consists of a CLI tool that can construct a database of parts. The CLI has multiple ingest sources, and each can be optionally run to construct a db incrementally. There may be several human-maintained kdl source files. Some ingest sources might just be a set of parts than can be determined by a pattern in a part series. Some ingest sources might be web APIs.

The database itself is just a directory of kdl files. The generated ones should have a comment at the top that they're generated. There will always be a copy of the db checked into the repo at ./db.
