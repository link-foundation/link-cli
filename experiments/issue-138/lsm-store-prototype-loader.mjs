// Redirect only the missing store entry in the isolated experiment.
export async function resolve(specifier, context, next) {
  if(specifier.endsWith('/src/engine-lsm-store.js'))return {url:new URL('./lsm-store-prototype.mjs',import.meta.url).href,shortCircuit:true};
  return next(specifier,context);
}
