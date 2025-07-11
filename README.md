# link-cli
`clink` (`CLInk` `cLINK`), a CLI tool to manipulate links using single substitution operation.

It is based on [associative theory](https://habr.com/ru/articles/895896) (also in [ru](https://habr.com/ru/articles/804617)) and [Links Notation](https://github.com/linksplatform/Protocols.Lino) (also in [ru](https://github.com/linksplatform/Protocols.Lino/blob/main/README.ru.md))

It uses C# implementation of [a links data store](https://github.com/linksplatform?view_as=public) (see also in [ru](https://github.com/linksplatform/.github/blob/main/profile/README.ru.md)).

This CLI tool can be installed globally as `clink` using single command (that will work if you have [.NET](https://dotnet.microsoft.com/en-us/download) installed):

```bash
dotnet tool install --global clink
```

<img width="811" alt="Screenshot 2025-05-16 at 5 48 06 AM" src="https://github.com/user-attachments/assets/615df4ce-658e-4bab-a483-96fae200f106" />


This tool provides all CRUD operations for links using single [substitution operation](https://en.wikipedia.org/wiki/Markov_algorithm) ([ru](https://ru.wikipedia.org/wiki/Нормальный_алгоритм)) which is turing complete.

Each operations split into two parts:

```
(matching pattern)
(substitution pattern)
```

When match pattern and substitution pattern are essensially the same we get no changes (no operation), it may seem like it does not any write, but it actually does the read operation.

For example when `--changes` option is enabled this operation:

```
((1: 1 1)) ((1: 1 1))
```

will output:

```
((1: 1 1)) ((1: 1 1))
```

That is change of 1-st link with start (source) at itself and end (target) at itself to itself. Meaning no change, but as match pattern applies only to the link with 1 as index, 1 as source and 1 as target, this "no change" can be used as read query.

Creation is just a replacement of nothing to something:

```
() ((1 1))
```

Where first `()` is just empty sequence of links, that symbolizes nothing. And `((1 1))` is a sequence of link with 1 as a start and 1 as end, the index is undefined so it for database to decide actual available id (index).

Deletion is just a replacement of something to nothing:

```
((1 1)) () 
```

Where `((1 1))` is a sequence of match patterns, with a single pattern for a link with 1 as a start and 1 as end, the index is undefined, meaning it can be any index. It will match only existing link, if no such link found there will be no match. Last `()` is just empty sequence of links, that symbolizes nothing. We don't have matched link on the right side, meaning it will be effectively deleted.

And the update is substitution itself, obviously.

```
((1: 1 1)) ((1: 1 2))
```

In that case we have a link with 1-st id on both sides, meaning it is not deleted and not created, it is changed. In this particular example with change the target of the link (its ending) to 2. 2 is ofcourse id of another link. In here we have only links, nothing else.

## Create single link

Create link with 1 as source and 1 as target.

```bash
clink '() ((1 1))' --changes --after
```
→
```
() ((1: 1 1))
(1: 1 1)
```

Create link with 2 as source and 2 as target.

```bash
clink '() ((2 2))' --changes --after
```
→
```
() ((2: 2 2))
(1: 1 1)
(2: 2 2)
```

## Create multiple links

Create two links at the same time: (1 1) and (2 2).

```bash
clink '() ((1 1) (2 2))' --changes --after
```
→
```
() ((2: 2 2))
() ((1: 1 1))
(1: 1 1)
(2: 2 2)
```

## Read all links

```bash
clink '((($i: $s $t)) (($i: $s $t)))' --changes --after
```
→
```
((1: 1 1)) ((1: 1 1))
((2: 2 2)) ((2: 2 2))
(1: 1 1)
(2: 2 2)
```

Where `$i` stands for variable named `i`, that stands for `index`. `$s` is for `source` and `$t` is for `target`.

A short version of read operation will also work:
```
clink '((($i:)) (($i:)))' --changes
```

## Update single link

Update link with index 1 and source 1 and target 1, changing target to 2.

```bash
clink '((1: 1 1)) ((1: 1 2))' --changes --after
```
→
```
((1: 1 1)) ((1: 1 2))
(1: 1 2)
(2: 2 2)
```

## Update multiple links

Update link with index 1 and source 1 and target 1, changing target to 2.

```bash
clink '((1: 1 1) (2: 2 2)) ((1: 1 2) (2: 2 1))' --changes --after
```
→
```
((1: 1 1)) ((1: 1 2))
((2: 2 2)) ((2: 2 1))
(1: 1 2)
(2: 2 1)
```

## Delete single link

Delete link with source 1 and target 2:

```bash
clink '((1 2)) ()' --changes --after
```
→
```
((1: 1 2)) ()
(2: 2 2)
```

Delete link with source 2 and target 2:

```bash
clink '((2 2)) ()' --changes --after
```
→
```
((2: 2 2)) ()
```

## Delete multiple links

```bash
clink '((1 2) (2 2)) ()' --changes --after
```
→
```
((1: 1 2)) ()
((2: 2 2)) ()
```

## Delete all links

```bash
clink '((* *)) ()' --changes --after
```
→
```
((1: 1 2)) ()
((2: 2 2)) ()
```

## Complete examples:

```bash
clink '() ((1 1) (2 2))' --changes --after

clink '((1: 1 1) (2: 2 2)) ((1: 1 2) (2: 2 1))' --changes --after

clink '((1 2) (2 1)) ()' --changes --after
```

```bash
clink '() ((1 2) (2 1))' --changes --after

clink '((($index: $source $target)) (($index: $target $source)))' --changes --after

clink '((1: 2 1) (2: 1 2)) ()' --changes --after
```

## All options and arguments

| Parameter               | Type    | Default Value  | Aliases                             | Description                                                                |
|-------------------------|---------|----------------|-------------------------------------|----------------------------------------------------------------------------|
| `--db`                  | string  | `db.links`     | `--data-source`, `--data`, `-d`     | Path to the links database file                                            |
| `--query`               | string  | _None_         | `--apply`, `--do`, `-q`             | LiNo query for CRUD operation                                              |
| `query` (positional)    | string  | _None_         | _N/A_                               | LiNo query for CRUD operation (provided as the first positional argument)  |
| `--trace`               | bool    | `false`        | `-t`                                | Enable trace (verbose output)                                              |
| `--structure`           | uint?   | _None_         | `-s`                                | ID of the link to format its structure                                     |
| `--before`              | bool    | `false`        | `-b`                                | Print the state of the database before applying changes                    |
| `--changes`             | bool    | `false`        | `-c`                                | Print the changes applied by the query                                     |
| `--after`               | bool    | `false`        | `--links`, `-a`                     | Print the state of the database after applying changes                     |

## For developers and debugging

### Execute from root

```bash
dotnet run --project Foundation.Data.Doublets.Cli -- '(((1: 1 1) (2: 2 2)) ((1: 1 2) (2: 2 1)))' --changes --after
```

### Execute from folder

```bash
cd Foundation.Data.Doublets.Cli
dotnet run -- '(((1: 1 1) (2: 2 2)) ((1: 1 2) (2: 2 1)))' --changes --after
```

### Complete examples:

```bash
dotnet run --project Foundation.Data.Doublets.Cli -- '() ((1 1) (2 2))' --changes --after

dotnet run --project Foundation.Data.Doublets.Cli -- '((1: 1 1) (2: 2 2)) ((1: 1 2) (2: 2 1))' --changes --after

dotnet run --project Foundation.Data.Doublets.Cli -- '((1 2) (2 1)) ()' --changes --after
```

```bash
dotnet run --project Foundation.Data.Doublets.Cli -- '() ((1 2) (2 1))' --changes --after

dotnet run --project Foundation.Data.Doublets.Cli -- '((($index: $source $target)) (($index: $target $source)))' --changes --after

dotnet run --project Foundation.Data.Doublets.Cli -- '((1: 2 1) (2: 1 2)) ()' --changes --after
```

### Publish next version:

```bash
VERSION=$(awk -F'[<>]' '/<Version>/ {print $3}' Foundation.Data.Doublets.Cli/Foundation.Data.Doublets.Cli.csproj) && git tag "v$VERSION" && git push origin "v$VERSION"
```

## Running a Specific Test with Detailed Output

To run a specific test (e.g., `DeleteAllLinksByIndexTest`) with detailed output, use:

```
dotnet test --filter "FullyQualifiedName=Foundation.Data.Doublets.Cli.Tests.Tests.AdvancedMixedQueryProcessor.DeleteAllLinksByIndexTest" --logger "console;verbosity=detailed"
```

This will execute only the specified test and show detailed logs in the console.

**Short version:**
```
dotnet test --filter DeleteAllLinksByIndexTest -v n
```
