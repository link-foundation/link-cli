# link-cli
A CLI tool to manipulate links.

## Create link

Create link with 1 as source and 1 as target.

```bash
dotnet run -- --query "(() ((1 1)))"
```
→
```
(1: 1 1)
```

Create link with 2 as source and 2 as target.

```bash
dotnet run -- --query "(() ((2 2)))"
```
→
```
(1: 1 1)
(2: 2 2)
```

## Update link

Update link with index 1 and source 1 and target 1, changing target to 2.

```bash
dotnet run -- --query "(((1: 1 1)) ((1: 1 2)))"
```
→
```
(1: 1 2)
(2: 2 2)
```

## Delete link

Delete link with source 1 and target 2:

```bash
dotnet run -- --query "(((1 2)) ())"
```
→
```
(2: 2 2)
```

Delete link with source 2 and target 2:

```bash
dotnet run -- --query "(((2 2)) ())"
```
→
```
```
