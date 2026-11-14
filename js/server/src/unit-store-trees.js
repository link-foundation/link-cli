// The two size-balanced trees and the free list of a doublets unit store:
// `LinksSourcesRecursionlessSizeBalancedTree`,
// `LinksTargetsRecursionlessSizeBalancedTree` and `UnusedLinks` of `doublets`
// 0.21 (src/mem/unit/generic/), over `IterativeSizeBalancedTree`,
// `RecursiveSizeBalancedTree` and `AbsoluteCircularLinkedList` of
// `platform-trees` 0.3.5.
//
// Every link with a non-null source is a node of the sources tree, ordered
// by `(source, target)`, and every link with a non-null target a node of the
// targets tree, ordered by `(target, source)`; the left, right and size of a
// node are three fields of its own `LinkPart`. The roots are header fields.
// A freed address is an element of a circular list whose previous and next
// pointers are the `source` and `target` of its item.
//
// Rust walks the trees through raw `*mut T` pointers to the root header
// field or to a node's left or right field; here such a pointer is the pair
// `(item, field)` of the memory. The steps are the upstream ones, in the
// same order, so the same operations leave the same bytes.
import { DoubletsError } from './unit-store-doublets.js';
import { Field, Header } from './unit-store-memory.js';

/** The fields one of the two trees uses. */
const SOURCES = Object.freeze({
  root: Header.RootAsSource,
  base: Field.Source,
  other: Field.Target,
  left: Field.LeftAsSource,
  right: Field.RightAsSource,
  size: Field.SizeAsSource,
});

const TARGETS = Object.freeze({
  root: Header.RootAsTarget,
  base: Field.Target,
  other: Field.Source,
  left: Field.LeftAsTarget,
  right: Field.RightAsTarget,
  size: Field.SizeAsTarget,
});

/** `(firstBase, firstOther) < (secondBase, secondOther)`. */
const isLeft = (firstBase, firstOther, secondBase, secondOther) =>
  firstBase < secondBase || (firstBase === secondBase && firstOther < secondOther);

/** `(firstBase, firstOther) > (secondBase, secondOther)`. */
const isRight = (firstBase, firstOther, secondBase, secondOther) =>
  firstBase > secondBase || (firstBase === secondBase && firstOther > secondOther);

/**
 * A recursionless size-balanced tree of links. `link(index)` turns an
 * address into the link a read handler receives.
 */
export class LinksTree {
  constructor(memory, fields, link) {
    this.memory = memory;
    this.fields = fields;
    this.link = link;
  }

  /** The sources tree of `memory`. */
  static sources(memory, link) {
    return new LinksTree(memory, SOURCES, link);
  }

  /** The targets tree of `memory`. */
  static targets(memory, link) {
    return new LinksTree(memory, TARGETS, link);
  }

  rootNode() {
    return this.memory.get(0, this.fields.root);
  }

  left(node) {
    return this.memory.get(node, this.fields.left);
  }

  right(node) {
    return this.memory.get(node, this.fields.right);
  }

  size(node) {
    return this.memory.get(node, this.fields.size);
  }

  setLeft(node, value) {
    this.memory.set(node, this.fields.left, value);
  }

  setRight(node, value) {
    this.memory.set(node, this.fields.right, value);
  }

  setSize(node, value) {
    this.memory.set(node, this.fields.size, value);
  }

  leftOrDefault(node) {
    return node === 0 ? 0 : this.left(node);
  }

  rightOrDefault(node) {
    return node === 0 ? 0 : this.right(node);
  }

  sizeOrZero(node) {
    return node === 0 ? 0 : this.size(node);
  }

  leftSize(node) {
    return this.sizeOrZero(this.leftOrDefault(node));
  }

  rightSize(node) {
    return this.sizeOrZero(this.rightOrDefault(node));
  }

  incSize(node) {
    this.setSize(node, this.size(node) + 1);
  }

  decSize(node) {
    this.setSize(node, this.size(node) - 1);
  }

  fixSize(node) {
    this.setSize(node, this.leftSize(node) + this.rightSize(node) + 1);
  }

  basePart(node) {
    return this.memory.get(node, this.fields.base);
  }

  otherPart(node) {
    return this.memory.get(node, this.fields.other);
  }

  /** `first_is_to_the_left_of_second` on two nodes. */
  firstIsLeft(first, second) {
    const { memory, fields } = this;
    return isLeft(
      memory.get(first, fields.base),
      memory.get(first, fields.other),
      memory.get(second, fields.base),
      memory.get(second, fields.other),
    );
  }

  /** `first_is_to_the_right_of_second` on two nodes. */
  firstIsRight(first, second) {
    const { memory, fields } = this;
    return isRight(
      memory.get(first, fields.base),
      memory.get(first, fields.other),
      memory.get(second, fields.base),
      memory.get(second, fields.other),
    );
  }

  /** `*root = left_rotate_core(*root)` for the pointer `(item, field)`. */
  leftRotate(item, field) {
    const root = this.memory.get(item, field);
    const right = this.right(root);
    this.setRight(root, this.left(right));
    this.setLeft(right, root);
    this.setSize(right, this.size(root));
    this.fixSize(root);
    this.memory.set(item, field, right);
  }

  /** `*root = right_rotate_core(*root)` for the pointer `(item, field)`. */
  rightRotate(item, field) {
    const root = this.memory.get(item, field);
    const left = this.left(root);
    this.setLeft(root, this.right(left));
    this.setRight(left, root);
    this.setSize(left, this.size(root));
    this.fixSize(root);
    this.memory.set(item, field, left);
  }

  rightest(node) {
    let current = node;
    for (let next = this.right(current); next !== 0; next = this.right(current)) current = next;
    return current;
  }

  leftest(node) {
    let current = node;
    for (let next = this.left(current); next !== 0; next = this.left(current)) current = next;
    return current;
  }

  clearNode(node) {
    this.setLeft(node, 0);
    this.setRight(node, 0);
    this.setSize(node, 0);
  }

  /** `LinksTree::attach` on the root header field. */
  attach(node) {
    if (this.rootNode() === 0) {
      this.setSize(node, 1);
      this.memory.set(0, this.fields.root, node);
      return;
    }
    this.attachCore(0, this.fields.root, node);
  }

  /** `LinksTree::detach` on the root header field. */
  detach(node) {
    this.detachCore(0, this.fields.root, node);
  }

  /** `IterativeSizeBalancedTree::attach_core` below the pointer `(item, field)`. */
  attachCore(item, field, node) {
    const { memory, fields } = this;
    for (;;) {
      const root = memory.get(item, field);
      const left = this.left(root);
      const leftSize = this.sizeOrZero(left);
      const right = this.right(root);
      const rightSize = this.sizeOrZero(right);
      if (this.firstIsLeft(node, root)) {
        if (left === 0) {
          this.incSize(root);
          this.setSize(node, 1);
          this.setLeft(root, node);
          return;
        }
        if (this.firstIsLeft(node, left)) {
          if (leftSize + 1 > rightSize) this.rightRotate(item, field);
          else {
            this.incSize(root);
            [item, field] = [root, fields.left];
          }
        } else {
          const leftRightSize = this.sizeOrZero(this.right(left));
          if (leftRightSize + 1 > rightSize) {
            if (leftRightSize === 0 && rightSize === 0) {
              this.setLeft(node, left);
              this.setRight(node, root);
              this.setSize(node, leftSize + 2);
              this.setLeft(root, 0);
              this.setSize(root, 1);
              memory.set(item, field, node);
              return;
            }
            this.leftRotate(root, fields.left);
            this.rightRotate(item, field);
          } else {
            this.incSize(root);
            [item, field] = [root, fields.left];
          }
        }
      } else {
        if (right === 0) {
          this.incSize(root);
          this.setSize(node, 1);
          this.setRight(root, node);
          return;
        }
        if (this.firstIsRight(node, right)) {
          if (rightSize + 1 > leftSize) this.leftRotate(item, field);
          else {
            this.incSize(root);
            [item, field] = [root, fields.right];
          }
        } else {
          const rightLeftSize = this.sizeOrZero(this.left(right));
          if (rightLeftSize + 1 > leftSize) {
            if (rightLeftSize === 0 && leftSize === 0) {
              this.setLeft(node, root);
              this.setRight(node, right);
              this.setSize(node, rightSize + 2);
              this.setRight(root, 0);
              this.setSize(root, 1);
              memory.set(item, field, node);
              return;
            }
            this.rightRotate(root, fields.right);
            this.leftRotate(item, field);
          } else {
            this.incSize(root);
            [item, field] = [root, fields.right];
          }
        }
      }
    }
  }

  /** `IterativeSizeBalancedTree::detach_core` below the pointer `(item, field)`. */
  detachCore(item, field, node) {
    const { memory, fields } = this;
    for (;;) {
      const root = memory.get(item, field);
      const left = this.left(root);
      const leftSize = this.sizeOrZero(left);
      const right = this.right(root);
      const rightSize = this.sizeOrZero(right);
      if (this.firstIsLeft(node, root)) {
        if (leftSize === 0) throw this.missing(node);
        const decrementedLeftSize = leftSize - 1;
        if (this.sizeOrZero(this.rightOrDefault(right)) > decrementedLeftSize) {
          this.leftRotate(item, field);
        } else if (this.sizeOrZero(this.leftOrDefault(right)) > decrementedLeftSize) {
          this.rightRotate(root, fields.right);
          this.leftRotate(item, field);
        } else {
          this.decSize(root);
          [item, field] = [root, fields.left];
        }
      } else if (this.firstIsRight(node, root)) {
        if (rightSize === 0) throw this.missing(node);
        const decrementedRightSize = rightSize - 1;
        if (this.sizeOrZero(this.leftOrDefault(left)) > decrementedRightSize) {
          this.rightRotate(item, field);
        } else if (this.sizeOrZero(this.rightOrDefault(left)) > decrementedRightSize) {
          this.leftRotate(root, fields.left);
          this.rightRotate(item, field);
        } else {
          this.decSize(root);
          [item, field] = [root, fields.right];
        }
      } else {
        if (leftSize > 0 && rightSize > 0) {
          let replacement;
          if (leftSize > rightSize) {
            replacement = this.rightest(left);
            this.detachCore(root, fields.left, replacement);
          } else {
            replacement = this.leftest(right);
            this.detachCore(root, fields.right, replacement);
          }
          this.setLeft(replacement, this.left(root));
          this.setRight(replacement, this.right(root));
          this.setSize(replacement, leftSize + rightSize);
          memory.set(item, field, replacement);
        } else if (leftSize > 0) memory.set(item, field, left);
        else if (rightSize > 0) memory.set(item, field, right);
        else memory.set(item, field, 0);
        this.clearNode(node);
        return;
      }
    }
  }

  /**
   * The size underflow a debug build of Rust panics on ("attempt to subtract
   * with overflow") when `node` is not where its parts say: the tree holds
   * two links with the same `(source, target)`, which only the uniqueness
   * resolution prevents.
   */
  missing(node) {
    const name = this.fields === SOURCES ? 'sources' : 'targets';
    const error = new DoubletsError(`link ${node} is missing from the ${name} tree`);
    return Object.assign(error, { corrupt: true });
  }

  /** `LinksTree::count_usages`: how many nodes have `link` as their base part. */
  countUsages(link) {
    let root = this.rootNode();
    const total = this.sizeOrZero(root);
    let rightIgnore = 0;
    while (root !== 0) {
      if (this.basePart(root) <= link) root = this.rightOrDefault(root);
      else {
        rightIgnore += this.rightSize(root) + 1;
        root = this.leftOrDefault(root);
      }
    }
    root = this.rootNode();
    let leftIgnore = 0;
    while (root !== 0) {
      if (this.basePart(root) >= link) root = this.leftOrDefault(root);
      else {
        leftIgnore += this.leftSize(root) + 1;
        root = this.rightOrDefault(root);
      }
    }
    return total - rightIgnore - leftIgnore;
  }

  /** `LinksTree::search` (on the sources tree): the node `(source, target)`, or 0. */
  search(source, target) {
    let root = this.rootNode();
    while (root !== 0) {
      const rootBase = this.basePart(root);
      const rootOther = this.otherPart(root);
      if (isLeft(source, target, rootBase, rootOther)) root = this.leftOrDefault(root);
      else if (isRight(source, target, rootBase, rootOther)) root = this.rightOrDefault(root);
      else return root;
    }
    return 0;
  }

  /**
   * `LinksTree::each_usages`: every node with `base` as its base part, in
   * pre-order. `handler` returns `false` to break; so does this.
   */
  eachUsages(base, handler) {
    return this.eachUsagesCore(base, this.rootNode(), handler);
  }

  eachUsagesCore(base, node, handler) {
    if (node === 0) return true;
    const part = this.basePart(node);
    if (part > base) return this.eachUsagesCore(base, this.leftOrDefault(node), handler);
    if (part < base) return this.eachUsagesCore(base, this.rightOrDefault(node), handler);
    return (
      handler(this.link(node)) !== false &&
      this.eachUsagesCore(base, this.leftOrDefault(node), handler) &&
      this.eachUsagesCore(base, this.rightOrDefault(node), handler)
    );
  }
}

/** `UnusedLinks`: the circular list of freed addresses, rooted in the header. */
export class UnusedLinks {
  constructor(memory) {
    this.memory = memory;
  }

  first() {
    return this.memory.get(0, Header.FirstFree);
  }

  previous(element) {
    return this.memory.get(element, Field.Source);
  }

  next(element) {
    return this.memory.get(element, Field.Target);
  }

  setFirst(element) {
    this.memory.set(0, Header.FirstFree, element);
  }

  setLast(element) {
    this.memory.set(0, Header.LastFree, element);
  }

  setPrevious(element, previous) {
    this.memory.set(element, Field.Source, previous);
  }

  setNext(element, next) {
    this.memory.set(element, Field.Target, next);
  }

  addSize(delta) {
    this.memory.set(0, Header.Free, this.memory.get(0, Header.Free) + delta);
  }

  /**
   * `AbsoluteCircularLinkedList::attach_as_first`, with its `attach_before`
   * of the first element inlined: that element is the first one, so the new
   * one always becomes the first.
   */
  attachAsFirst(element) {
    const first = this.first();
    if (first === 0) {
      this.setFirst(element);
      this.setLast(element);
      this.setPrevious(element, element);
      this.setNext(element, element);
    } else {
      const last = this.previous(first);
      this.setPrevious(element, last);
      this.setNext(element, first);
      this.setFirst(element);
      this.setNext(last, element);
      this.setPrevious(first, element);
    }
    this.addSize(1);
  }

  /** `AbsoluteCircularLinkedList::detach`. */
  detach(element) {
    const previous = this.previous(element);
    const next = this.next(element);
    if (next === element) {
      this.setFirst(0);
      this.setLast(0);
    } else {
      this.setNext(previous, next);
      this.setPrevious(next, previous);
      if (element === this.first()) this.setFirst(next);
      if (element === this.memory.get(0, Header.LastFree)) this.setLast(previous);
    }
    this.setPrevious(element, 0);
    this.setNext(element, 0);
    this.addSize(-1);
  }
}
