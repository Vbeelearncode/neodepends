# NeoDepends Python Dependencies

This document describes the dependency kinds NeoDepends emits for Python, and how the enhancement step expands the raw extraction.

## Core dependency kinds (DV8/DSM)

- **Import**
  - File/module imports another module.
- **Extend**
  - Class inherits from another class.
- **Create**
  - A method/function instantiates a class (e.g., `ClassName(...)`).
- **Call**
  - A method/function calls another method/function.
- **Use**
  - A method/function accesses a field/attribute.
- **Override**
  - A concrete method implements an abstract method from a base class.

These are the kinds used in handcount comparisons and the exported DV8 DSMs.

## Enhancement step (Python)

The Python enhancement step (`tools/enhance_python_deps.py`) adds dependencies that are not captured by raw extraction alone. It focuses on structural dependencies that are important for architectural analysis and handcount alignment.

### 1) Method -> Field (Use)
Detects `self.field` access inside methods and records it as a **Use** edge:

- `Class.method -> Class.field (Use)`

This is critical for clustering and architectural signal.

### 2) Field -> Field (Use)  **(new)**
When a field is defined in terms of another field (for example, `self.a = self.b` or `self.a = self.b + 1`), we add a **Use** edge between fields:

- `Class.field_a -> Class.field_b (Use)`

This captures intra-class data dependencies that otherwise disappear after field-parent normalization.

### 3) Method -> Function (Call)  **(new)**
For module-level functions called inside methods, we add a **Call** edge:

- `Class.method -> module.function (Call)`

Previously only function->function calls were guaranteed; now method->function calls are explicit.

### 4) Override detection (abstract methods)
If a base class defines abstract methods (ABC), and a subclass implements them, we add **Override** edges:

- `Subclass.method -> BaseClass.method (Override)`

This is equivalent to Java `@Override` semantics for Python ABCs.

## Notes on naming / normalization

Handcount comparisons normalize names (file paths, class/method formatting) to align DV8 output with the handcount convention. This is done by `tools/compare_dv8_to_ground_truth.py` and does **not** change the underlying dependencies.

## ADVANCED: Abstract classes & duck typing

### Abstract Classes (Static typing in Python)
Abstract base classes (ABCs) define method signatures without implementation. Child classes must implement them.

Example:

from abc import ABC, abstractmethod

class Vehicle(ABC):
    @abstractmethod
    def start_engine(self):
        pass

class Car(Vehicle):
    def start_engine(self):
        print("Car engine started")

Dependency meaning: renaming `Vehicle.start_engine()` forces changes to `Car.start_engine()`.

### Duck Typing (Dynamic typing)
Python allows implicit interfaces without inheritance. Two classes with matching method names can be used interchangeably.

Example:

class Dog:
    def speak(self): return "Woof"

class Cat:
    def speak(self): return "Meow"

def make_it_speak(thing):
    thing.speak()

Duck-typing dependencies are real but **hard to detect** statically without type inference or runtime tracing. NeoDepends does not currently emit duck-typing edges.

### What exists in the toy examples

- Abstract classes: **Yes** (Python SECOND, Java SECOND)
- Duck typing: **No**

NeoDepends detects abstract-method override edges in Python and Java as **Override** dependencies.
