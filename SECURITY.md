# Security policy

## Supported versions

| Version | Supported |
|---|---|
| 1.x | No: the project is archived and receives no more updates |

## Scope

L'Arène des générations is a **local experiment**. By default the server listens on `127.0.0.1:8080`, local requests are logged in automatically, and there is no TLS. These choices are deliberate.

What matters if you run or fork it:

- Do not expose the interface beyond your machine without changing `adresse_serveur`, putting TLS in front of it and using the referee key (`donnees/cle-admin.txt`). Automatic local login never applies through a tunnel (`X-Forwarded-For` is not trusted).
- The Files page can edit the configuration, the page templates and the run results: anyone with the referee key controls the Arena.
- Agent tools run model-generated code and fetch web pages. Python runs in a throwaway Docker container without network, without file access, read-only, with 256 MB and a time limit; web search refuses local and private addresses. Keep these limits if you change the tools.

## Reporting a vulnerability

The repository is archived, so no fix will be released. If you find a serious issue that could affect people running the code as is, you can still contact the maintainer, [@rjvdev](https://github.com/rjvdev), through GitHub. Please do not publish exploit details first.
