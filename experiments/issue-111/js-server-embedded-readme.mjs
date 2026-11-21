// Checks the embedded-store snippet of js/server/README.md.
import { NamedTypesDecorator, QueryProcessor } from '../../js/server/src/index.js';

const directory = process.argv[2] ?? '.';
const store = NamedTypesDecorator.open(`${directory}/family.links`);
const processor = new QueryProcessor().withAutoCreateMissingReferences(true);
const changes = processor.processQuery(store, '() ((child: father mother))');
for (const [before, after] of changes) store.printChangeLino(before, after, process.stdout);
store.printAllLino(process.stdout);
console.log(store.getByName('child'), store.getName(3));
store.save();
