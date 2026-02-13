# Absolute Priority Premise (APP)

## Description

APP assigns "mass" values to code components. Lower mass
indicates simpler code. Use during refactoring to compare
equivalent solutions.

## The Six Components (with Mass Values)

### 1. Constant (Mass: 1)

- Literal values: numbers, strings, booleans, empty
  collections, absence markers
- Lowest mass - preferred building block

### 2. Binding/Scalar (Mass: 1)

- Variables, parameters, local names
- Names that refer to values

### 3. Invocation (Mass: 2)

- Function or method calls
- Calling existing functionality

### 4. Conditional (Mass: 4)

- Control flow decisions: branches, pattern matching,
  ternary expressions
- Branching logic

### 5. Loop (Mass: 5)

- Iteration constructs: explicit loops and transforming
  higher-order functions (map, filter, flat-map, etc.)
- Repetitive execution
- **Note**: Only transforming operations count as loops;
  consuming/terminal operations count as invocations.
  See [Transforming vs Consuming](#transforming-vs-consuming)

### 6. Assignment (Mass: 6)

- Mutating existing variables or appending to mutable
  collections
- State changes - most complex

## Calculation Rules

### Total Mass = Sum of All Components

```text
Total Mass = (constants * 1) + (bindings * 1)
           + (invocations * 2) + (conditionals * 4)
           + (loops * 5) + (assignments * 6)
```

### Comparison Guidelines

- **Lower mass = Better code**
- **Functional style naturally scores lower**
  (no assignments or loops)
- **Immutable approaches preferred** over mutable ones
- **Simple expressions preferred** over complex control
  structures

## Special Counting Rules

### Function Declarations

- A function counts as both:
  - **Constant (1)** for the code it represents
  - **Binding (1)** for its name

### Assignment Distinctions

- **Not every assignment counts as Assignment (6)**
- Initial bindings without mutation are **Bindings (1)**
- Only **re-assignments that modify values** count as
  Assignment (6)
- A mutable variable that gets mutated counts as
  **Binding (1) + Assignment (6)**

### Transforming vs Consuming

Distinguish between transforming and consuming collection
operations:

- **Transforming** operations define iteration behavior
  and count as **Loops (5)**: map, filter, flat-map, take,
  skip, zip, chain, fold, for-each
- **Consuming** operations trigger and collect results
  and count as **Invocations (2)**: collect/to-list, sum,
  count, any, all, find, min, max, first, last

### Type Definitions

- Type/class definition = **Constant (1)** for code
  + **Binding (1)** for name
- Usually ignored in algorithm comparisons

## Example: Validation

### Imperative Validation (Higher Mass)

```pseudocode
function validate(request):
    errors = []
    if request.id < 0:
        errors.push(NegativeId)
    if request.items.isEmpty():
        errors.push(NoItems)
    if errors.isEmpty():
        return Ok()
    else:
        return Err(errors)
```

Total Mass: 46

### Early Return Validation (Lower Mass)

```pseudocode
function validate(request):
    if request.id < 0:           // conditional(4)
                                 //   + invocation(2) + constant(1)
        return Err(NegativeId)   // invocation(2)

    if request.items.isEmpty():  // conditional(4) + invocation(2)
        return Err(NoItems)      // invocation(2)

    return Ok()                  // invocation(2)
```

Total Mass: 23

Early return eliminates all assignments and one conditional,
cutting mass by ~50%.

> **Agents**: When demonstrating APP to the user, write
> comparison examples in the project's primary language.

## When to Use

- **During refactoring** to choose between equivalent
  solutions
- **Comparing algorithms** of similar functionality
- **NOT during initial TDD** red/green phases

## Limitations

- **Clarity always trumps mass** - readability matters most
- **Ignores performance** - doesn't measure speed or memory
- **Context-dependent** - works best for algorithm
  comparisons

## Remember

- APP is a **tool, not a rule**
- **Context matters** more than absolute scores
- **Combine with other principles** for best results
- **Lower mass is generally better**, but not always
- **Use during refactoring**, not initial development
- **The best specific mass values are unknown** - these
  are suggested weights

## Sources and References

### Primary Sources

- **Micah Martin's original work** (2012) -
  [Transformation Priority Premise Applied][tpp-applied] -
  8th Light blog post with Coin Changer Kata example
- **8th Light University presentations** - Part One
  and Part Two

[tpp-applied]: https://8thlight.com/insights/transformation-priority-premise-applied

### Secondary Sources

- **Peter Kofler's detailed analysis** -
  [Absolute Priority Premise, an Example][app-example] -
  comprehensive explanation with Word Wrap kata examples

[app-example]: https://blog.code-cop.org/2016/08/absolute-priority-premise-example.html
