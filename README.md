# link-cli
A CLI tool to manipulate links.

## Run

```bash
dotnet run -- --query "(((1: 1 1)) ((1: 1 2)))"
```

Should output something like this:

```
Final data store contents:
(1: 1 2)
(2: 2 2)
```
