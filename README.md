# link-cli
`clink` (`CLInk` `cLINK`), a CLI tool to manipulate links.

It is based on [associative theory (in Russian)](https://habr.com/ru/companies/deepfoundation/articles/804617/) and [Links Notation](https://github.com/linksplatform/Protocols.Lino) ([ru](https://github.com/linksplatform/Protocols.Lino/blob/main/README.ru.md))

[Short description in English in what links are](https://github.com/linksplatform?view_as=public). And [in Russian](https://github.com/linksplatform/.github/blob/main/profile/README.ru.md).

<img width="777" alt="Screenshot 2024-12-05 at 15 22 12" src="https://github.com/user-attachments/assets/01d35e39-4bfd-4639-a457-fa86840e2bb8" />

## Execute from root

```bash
dotnet run --project Foundation.Data.Doublets.Cli -- --query "(((1: 1 1) (2: 2 2)) ((1: 1 2) (2: 2 1)))"
```

## Execute from folder

```bash
cd Foundation.Data.Doublets.Cli
dotnet run -- --query "(((1: 1 1) (2: 2 2)) ((1: 1 2) (2: 2 1)))"
```

## Execute from NuGet

```bash
dotnet tool install --global clink
```

## Create single link

Create link with 1 as source and 1 as target.

```bash
clink --query "() ((1 1))"
```
→
```
(1: 1 1)
```

Create link with 2 as source and 2 as target.

```bash
clink --query "() ((2 2))"
```
→
```
(1: 1 1)
(2: 2 2)
```

## Create multiple links

Create two links at the same time: (1 1) and (2 2).

```bash
clink --query "() ((1 1) (2 2))"
```
→
```
(1: 1 1)
(2: 2 2)
```

## Update single link

Update link with index 1 and source 1 and target 1, changing target to 2.

```bash
clink --query "((1: 1 1)) ((1: 1 2))"
```
→
```
(1: 1 2)
(2: 2 2)
```

## Update multiple links

Update link with index 1 and source 1 and target 1, changing target to 2.

```bash
clink --query "((1: 1 1) (2: 2 2)) ((1: 1 2) (2: 2 1))"
```
→
```
(1: 1 2)
(2: 2 1)
```

## Delete single link

Delete link with source 1 and target 2:

```bash
clink --query "((1 2)) ()"
```
→
```
(2: 2 2)
```

Delete link with source 2 and target 2:

```bash
clink --query "((2 2)) ()"
```
→
```
```

## Delete multiple links

```bash
clink --query "((1 2) (2 2)) ()"
```
→
```
```

## Complete example:

```bash
dotnet tool install --global clink

clink --query "() ((1 1) (2 2))"

clink --query "((1: 1 1) (2: 2 2)) ((1: 1 2) (2: 2 1))"

clink --query "((1 2) (2 1)) ()"
```


## Complete example for debug:

```bash
dotnet run --project Foundation.Data.Doublets.Cli -- --query "() ((1 1) (2 2))"

dotnet run --project Foundation.Data.Doublets.Cli -- --query "((1: 1 1) (2: 2 2)) ((1: 1 2) (2: 2 1))"

dotnet run --project Foundation.Data.Doublets.Cli -- --query "((1 2) (2 1)) ()"
```
