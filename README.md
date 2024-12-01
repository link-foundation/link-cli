# link-cli
A CLI tool to manipulate links.

## Create link

```bash
dotnet run -- --query "(() ((1 1)))"
```
Should output:
```
(1: 1 1)
```

```bash
dotnet run -- --query "(() ((2 2)))"
```
Should output
```
(1: 1 1)
(2: 2 2)
```

## Update link

```bash
dotnet run -- --query "(((1: 1 1)) ((1: 1 2)))"
```

Should output:

```
(1: 1 2)
(2: 2 2)
```
