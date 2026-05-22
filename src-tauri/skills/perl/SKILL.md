---
name: perl-patterns
description: Modern Perl 5.36+ idioms, best practices, and conventions for building robust, maintainable Perl applications.
origin: ECC
---

# Modern Perl Development Patterns

Idiomatic Perl 5.36+ patterns and best practices for building robust, maintainable applications.

## When to Activate

- Writing new Perl code or modules
- Reviewing Perl code for idiom compliance
- Refactoring legacy Perl to modern standards
- Designing Perl module architecture
- Migrating pre-5.36 code to modern Perl

## How It Works

Apply these patterns as a bias toward modern Perl 5.36+ defaults: signatures, explicit modules, focused error handling, and testable boundaries. The examples below are meant to be copied as starting points, then tightened for the actual app, dependency stack, and deployment model in front of you.

## Core Principles

### 1. Use `v5.36` Pragma

A single `use v5.36` replaces the old boilerplate and enables strict, warnings, and subroutine signatures.

```perl
# Good: Modern preamble
use v5.36;

sub greet($name) {
    say "Hello, $name!";
}

# Bad: Legacy boilerplate
use strict;
use warnings;
use feature 'say', 'signatures';
no warnings 'experimental::signatures';

sub greet {
    my ($name) = @_;
    say "Hello, $name!";
}
```

### 2. Subroutine Signatures

Use signatures for clarity and automatic arity checking.

```perl
use v5.36;

# Good: Signatures with defaults
sub connect_db($host, $port = 5432, $timeout = 30) {
    # $host is required, others have defaults
    return DBI->connect("dbi:Pg:host=$host;port=$port", undef, undef, {
        RaiseError => 1,
        PrintError => 0,
    });
}

# Good: Slurpy parameter for variable args
sub log_message($level, @details) {
    say "[$level] " . join(' ', @details);
}

# Bad: Manual argument unpacking
sub connect_db {
    my ($host, $port, $timeout) = @_;
    $port    //= 5432;
    $timeout //= 30;
    # ...
}
```

### 3. Context Sensitivity

Understand scalar vs list context — a core Perl concept.

```perl
use v5.36;

my @items = (1, 2, 3, 4, 5);

my @copy  = @items;            # List context: all elements
my $count = @items;            # Scalar context: count (5)
say "Items: " . scalar @items; # Force scalar context
```

### 4. Postfix Dereferencing

Use postfix dereference syntax for readability with nested structures.

```perl
use v5.36;

my $data = {
    users => [
        { name => 'Alice', roles => ['admin', 'user'] },
        { name => 'Bob',   roles => ['user'] },
    ],
};

# Good: Postfix dereferencing
my @users = $data->{users}->@*;
my @roles = $data->{users}[0]{roles}->@*;
my %first = $data->{users}[0]->%*;

# Bad: Circumfix dereferencing (harder to read in chains)
my @users = @{ $data->{users} };
my @roles = @{ $data->{users}[0]{roles} };
```

### 5. The `isa` Operator (5.32+)

Infix type-check — replaces `blessed($o) && $o->isa('X')`.

```perl
use v5.36;
if ($obj isa 'My::Class') { $obj->do_something }
```

## Error Handling

### eval/die Pattern

```perl
use v5.36;

sub parse_config($path) {
    my $content = eval { path($path)->slurp_utf8 };
    die "Config error: $@" if $@;
    return decode_json($content);
}
```

### Try::Tiny (Reliable Exception Handling)

```perl
use v5.36;
use Try::Tiny;

sub fetch_user($id) {
    my $user = try {
        $db->resultset('User')->find($id)
            // die "User $id not found\n";
    }
    catch {
        warn "Failed to fetch user $id: $_";
        undef;
    };
    return $user;
}
```

### Native try/catch (5.40+)

```perl
use v5.40;

sub divide($x, $y) {
    try {
        die "Division by zero" if $y == 0;
        return $x / $y;
    }
    catch ($e) {
        warn "Error: $e";
        return;
    }
}
```

## Modern OO with Moo

Prefer Moo for lightweight, modern OO. Use Moose only when its metaprotocol is needed.

```perl
# Good: Moo class
package User;
use Moo;
use Types::Standard qw(Str Int ArrayRef);
use namespace::autoclean;

has name  => (is => 'ro', isa => Str, required => 1);
has email => (is => 'ro', isa => Str, required => 1);
has age   => (is => 'ro', isa => Int, default  => sub { 0 });
has roles => (is => 'ro', isa => ArrayRef[Str], default => sub { [] });

sub is_admin($self) {
    return grep { $_ eq 'admin' } $self->roles->@*;
}

sub greet($self) {
    return "Hello, I'm " . $self->name;
}

1;

# Usage
my $user = User->new(
    name  => 'Alice',
    email => 'alice@example.com',
    roles => ['admin', 'user'],
);

# Bad: Blessed hashref (no validation, no accessors)
package User;
sub new {
    my ($class, %args) = @_;
    return bless \%args, $class;
}
sub name { return $_[0]->{name} }
1;
```

### Moo Roles

```perl
package Role::Serializable;
use Moo::Role;
use JSON::MaybeXS qw(encode_json);
requires 'TO_HASH';
sub to_json($self) { encode_json($self->TO_HASH) }
1;

package User;
use Moo;
with 'Role::Serializable';
has name  => (is => 'ro', required => 1);
has email => (is => 'ro', required => 1);
sub TO_HASH($self) { { name => $self->name, email => $self->email } }
1;
```

### Native `class` Keyword (5.38+, Corinna)

```perl
use v5.38;
use feature 'class';
no warnings 'experimental::class';

class Point {
    field $x :param;
    field $y :param;
    method magnitude() { sqrt($x**2 + $y**2) }
}

my $p = Point->new(x => 3, y => 4);
say $p->magnitude;  # 5
```

## Regular Expressions

### Named Captures and `/x` Flag

```perl
use v5.36;

# Good: Named captures with /x for readability
my $log_re = qr{
    ^ (?<timestamp> \d{4}-\d{2}-\d{2} \s \d{2}:\d{2}:\d{2} )
    \s+ \[ (?<level> \w+ ) \]
    \s+ (?<message> .+ ) $
}x;

if ($line =~ $log_re) {
    say "Time: $+{timestamp}, Level: $+{level}";
    say "Message: $+{message}";
}

# Bad: Positional captures (hard to maintain)
if ($line =~ /^(\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2})\s+\[(\w+)\]\s+(.+)$/) {
    say "Time: $1, Level: $2";
}
```

### Precompiled Patterns

```perl
use v5.36;

# Good: Compile once, use many
my $email_re = qr/^[A-Za-z0-9._%+-]+\@[A-Za-z0-9.-]+\.[A-Za-z]{2,}$/;

sub validate_emails(@emails) {
    return grep { $_ =~ $email_re } @emails;
}
```

## Data Structures

### References and Safe Deep Access

```perl
use v5.36;

# Hash and array references
my $config = {
    database => {
        host => 'localhost',
        port => 5432,
        options => ['utf8', 'sslmode=require'],
    },
};

# Safe deep access (returns undef if any level missing)
my $port = $config->{database}{port};           # 5432
my $missing = $config->{cache}{host};           # undef, no error

# Hash slices
my %subset;
@subset{qw(host port)} = @{$config->{database}}{qw(host port)};

# Array slices
my @first_two = $config->{database}{options}->@[0, 1];

# Multi-variable for loop (experimental in 5.36, stable in 5.40)
use feature 'for_list';
no warnings 'experimental::for_list';
for my ($key, $val) (%$config) {
    say "$key => $val";
}
```

## File I/O

### Three-Argument Open

```perl
use v5.36;

# Good: Three-arg open with autodie (core module, eliminates 'or die')
use autodie;

sub read_file($path) {
    open my $fh, '<:encoding(UTF-8)', $path;
    local $/;
    my $content = <$fh>;
    close $fh;
    return $content;
}

# Bad: Two-arg open (shell injection risk, see perl-security)
open FH, $path;            # NEVER do this
open FH, "< $path";        # Still bad — user data in mode string
```

### Path::Tiny for File Operations

```perl
use v5.36;
use Path::Tiny;

my $file = path('config', 'app.json');
my $content = $file->slurp_utf8;
$file->spew_utf8($new_content);

# Iterate directory
for my $child (path('src')->children(qr/\.pl$/)) {
    say $child->basename;
}
```

## Module Organization

### Standard Project Layout

```text
MyApp/
├── lib/
│   └── MyApp/
│       ├── App.pm           # Main module
│       ├── Config.pm        # Configuration
│       ├── DB.pm            # Database layer
│       └── Util.pm          # Utilities
├── bin/
│   └── myapp                # Entry-point script
├── t/
│   ├── 00-load.t            # Compilation tests
│   ├── unit/                # Unit tests
│   └── integration/         # Integration tests
├── cpanfile                 # Dependencies
├── Makefile.PL              # Build system
└── .perlcriticrc            # Linting config
```

### Exporter Patterns

```perl
package MyApp::Util;
use v5.36;
use Exporter 'import';

our @EXPORT_OK   = qw(trim);
our %EXPORT_TAGS = (all => \@EXPORT_OK);

sub trim($str) { $str =~ s/^\s+|\s+$//gr }

1;
```

## Tooling

### perltidy Configuration (.perltidyrc)

```text
-i=4        # 4-space indent
-l=100      # 100-char line length
-ci=4       # continuation indent
-ce         # cuddled else
-bar        # opening brace on same line
-nolq       # don't outdent long quoted strings
```

### perlcritic Configuration (.perlcriticrc)

```ini
severity = 3
theme = core + pbp + security

[InputOutput::RequireCheckedSyscalls]
functions = :builtins
exclude_functions = say print

[Subroutines::ProhibitExplicitReturnUndef]
severity = 4

[ValuesAndExpressions::ProhibitMagicNumbers]
allowed_values = 0 1 2 -1
```

### Dependency Management (cpanfile + carton)

```bash
cpanm App::cpanminus Carton   # Install tools
carton install                 # Install deps from cpanfile
carton exec -- perl bin/myapp  # Run with local deps
```

```perl
# cpanfile
requires 'Moo', '>= 2.005';
requires 'Path::Tiny';
requires 'JSON::MaybeXS';
requires 'Try::Tiny';

on test => sub {
    requires 'Test2::V0';
    requires 'Test::MockModule';
};
```

## Quick Reference: Modern Perl Idioms

| Legacy Pattern | Modern Replacement |
|---|---|
| `use strict; use warnings;` | `use v5.36;` |
| `my ($x, $y) = @_;` | `sub foo($x, $y) { ... }` |
| `@{ $ref }` | `$ref->@*` |
| `%{ $ref }` | `$ref->%*` |
| `open FH, "< $file"` | `open my $fh, '<:encoding(UTF-8)', $file` |
| `blessed hashref` | `Moo` class with types |
| `$1, $2, $3` | `$+{name}` (named captures) |
| `eval { }; if ($@)` | `Try::Tiny` or native `try/catch` (5.40+) |
| `BEGIN { require Exporter; }` | `use Exporter 'import';` |
| Manual file ops | `Path::Tiny` |
| `blessed($o) && $o->isa('X')` | `$o isa 'X'` (5.32+) |
| `builtin::true / false` | `use builtin 'true', 'false';` (5.36+, experimental) |

## Anti-Patterns

```perl
# 1. Two-arg open (security risk)
open FH, $filename;                     # NEVER

# 2. Indirect object syntax (ambiguous parsing)
my $obj = new Foo(bar => 1);            # Bad
my $obj = Foo->new(bar => 1);           # Good

# 3. Excessive reliance on $_
map { process($_) } grep { validate($_) } @items;  # Hard to follow
my @valid = grep { validate($_) } @items;           # Better: break it up
my @results = map { process($_) } @valid;

# 4. Disabling strict refs
no strict 'refs';                        # Almost always wrong
${"My::Package::$var"} = $value;         # Use a hash instead

# 5. Global variables as configuration
our $TIMEOUT = 30;                       # Bad: mutable global
use constant TIMEOUT => 30;              # Better: constant
# Best: Moo attribute with default

# 6. String eval for module loading
eval "require $module";                  # Bad: code injection risk
eval "use $module";                      # Bad
use Module::Runtime 'require_module';    # Good: safe module loading
require_module($module);
```

**Remember**: Modern Perl is clean, readable, and safe. Let `use v5.36` handle the boilerplate, use Moo for objects, and prefer CPAN's battle-tested modules over hand-rolled solutions.


---

---
name: perl-testing
description: Perl testing patterns using Test2::V0, Test::More, prove runner, mocking, coverage with Devel::Cover, and TDD methodology.
origin: ECC
---

# Perl Testing Patterns

Comprehensive testing strategies for Perl applications using Test2::V0, Test::More, prove, and TDD methodology.

## When to Activate

- Writing new Perl code (follow TDD: red, green, refactor)
- Designing test suites for Perl modules or applications
- Reviewing Perl test coverage
- Setting up Perl testing infrastructure
- Migrating tests from Test::More to Test2::V0
- Debugging failing Perl tests

## TDD Workflow

Always follow the RED-GREEN-REFACTOR cycle.

```perl
# Step 1: RED — Write a failing test
# t/unit/calculator.t
use v5.36;
use Test2::V0;

use lib 'lib';
use Calculator;

subtest 'addition' => sub {
    my $calc = Calculator->new;
    is($calc->add(2, 3), 5, 'adds two numbers');
    is($calc->add(-1, 1), 0, 'handles negatives');
};

done_testing;

# Step 2: GREEN — Write minimal implementation
# lib/Calculator.pm
package Calculator;
use v5.36;
use Moo;

sub add($self, $a, $b) {
    return $a + $b;
}

1;

# Step 3: REFACTOR — Improve while tests stay green
# Run: prove -lv t/unit/calculator.t
```

## Test::More Fundamentals

The standard Perl testing module — widely used, ships with core.

### Basic Assertions

```perl
use v5.36;
use Test::More;

# Plan upfront or use done_testing
# plan tests => 5;  # Fixed plan (optional)

# Equality
is($result, 42, 'returns correct value');
isnt($result, 0, 'not zero');

# Boolean
ok($user->is_active, 'user is active');
ok(!$user->is_banned, 'user is not banned');

# Deep comparison
is_deeply(
    $got,
    { name => 'Alice', roles => ['admin'] },
    'returns expected structure'
);

# Pattern matching
like($error, qr/not found/i, 'error mentions not found');
unlike($output, qr/password/, 'output hides password');

# Type check
isa_ok($obj, 'MyApp::User');
can_ok($obj, 'save', 'delete');

done_testing;
```

### SKIP and TODO

```perl
use v5.36;
use Test::More;

# Skip tests conditionally
SKIP: {
    skip 'No database configured', 2 unless $ENV{TEST_DB};

    my $db = connect_db();
    ok($db->ping, 'database is reachable');
    is($db->version, '15', 'correct PostgreSQL version');
}

# Mark expected failures
TODO: {
    local $TODO = 'Caching not yet implemented';
    is($cache->get('key'), 'value', 'cache returns value');
}

done_testing;
```

## Test2::V0 Modern Framework

Test2::V0 is the modern replacement for Test::More — richer assertions, better diagnostics, and extensible.

### Why Test2?

- Superior deep comparison with hash/array builders
- Better diagnostic output on failures
- Subtests with cleaner scoping
- Extensible via Test2::Tools::* plugins
- Backward-compatible with Test::More tests

### Deep Comparison with Builders

```perl
use v5.36;
use Test2::V0;

# Hash builder — check partial structure
is(
    $user->to_hash,
    hash {
        field name  => 'Alice';
        field email => match(qr/\@example\.com$/);
        field age   => validator(sub { $_ >= 18 });
        # Ignore other fields
        etc();
    },
    'user has expected fields'
);

# Array builder
is(
    $result,
    array {
        item 'first';
        item match(qr/^second/);
        item DNE();  # Does Not Exist — verify no extra items
    },
    'result matches expected list'
);

# Bag — order-independent comparison
is(
    $tags,
    bag {
        item 'perl';
        item 'testing';
        item 'tdd';
    },
    'has all required tags regardless of order'
);
```

### Subtests

```perl
use v5.36;
use Test2::V0;

subtest 'User creation' => sub {
    my $user = User->new(name => 'Alice', email => 'alice@example.com');
    ok($user, 'user object created');
    is($user->name, 'Alice', 'name is set');
    is($user->email, 'alice@example.com', 'email is set');
};

subtest 'User validation' => sub {
    my $warnings = warns {
        User->new(name => '', email => 'bad');
    };
    ok($warnings, 'warns on invalid data');
};

done_testing;
```

### Exception Testing with Test2

```perl
use v5.36;
use Test2::V0;

# Test that code dies
like(
    dies { divide(10, 0) },
    qr/Division by zero/,
    'dies on division by zero'
);

# Test that code lives
ok(lives { divide(10, 2) }, 'division succeeds') or note($@);

# Combined pattern
subtest 'error handling' => sub {
    ok(lives { parse_config('valid.json') }, 'valid config parses');
    like(
        dies { parse_config('missing.json') },
        qr/Cannot open/,
        'missing file dies with message'
    );
};

done_testing;
```

## Test Organization and prove

### Directory Structure

```text
t/
├── 00-load.t              # Verify modules compile
├── 01-basic.t             # Core functionality
├── unit/
│   ├── config.t           # Unit tests by module
│   ├── user.t
│   └── util.t
├── integration/
│   ├── database.t
│   └── api.t
├── lib/
│   └── TestHelper.pm      # Shared test utilities
└── fixtures/
    ├── config.json        # Test data files
    └── users.csv
```

### prove Commands

```bash
# Run all tests
prove -l t/

# Verbose output
prove -lv t/

# Run specific test
prove -lv t/unit/user.t

# Recursive search
prove -lr t/

# Parallel execution (8 jobs)
prove -lr -j8 t/

# Run only failing tests from last run
prove -l --state=failed t/

# Colored output with timer
prove -l --color --timer t/

# TAP output for CI
prove -l --formatter TAP::Formatter::JUnit t/ > results.xml
```

### .proverc Configuration

```text
-l
--color
--timer
-r
-j4
--state=save
```

## Fixtures and Setup/Teardown

### Subtest Isolation

```perl
use v5.36;
use Test2::V0;
use File::Temp qw(tempdir);
use Path::Tiny;

subtest 'file processing' => sub {
    # Setup
    my $dir = tempdir(CLEANUP => 1);
    my $file = path($dir, 'input.txt');
    $file->spew_utf8("line1\nline2\nline3\n");

    # Test
    my $result = process_file("$file");
    is($result->{line_count}, 3, 'counts lines');

    # Teardown happens automatically (CLEANUP => 1)
};
```

### Shared Test Helpers

Place reusable helpers in `t/lib/TestHelper.pm` and load with `use lib 't/lib'`. Export factory functions like `create_test_db()`, `create_temp_dir()`, and `fixture_path()` via `Exporter`.

## Mocking

### Test::MockModule

```perl
use v5.36;
use Test2::V0;
use Test::MockModule;

subtest 'mock external API' => sub {
    my $mock = Test::MockModule->new('MyApp::API');

    # Good: Mock returns controlled data
    $mock->mock(fetch_user => sub ($self, $id) {
        return { id => $id, name => 'Mock User', email => 'mock@test.com' };
    });

    my $api = MyApp::API->new;
    my $user = $api->fetch_user(42);
    is($user->{name}, 'Mock User', 'returns mocked user');

    # Verify call count
    my $call_count = 0;
    $mock->mock(fetch_user => sub { $call_count++; return {} });
    $api->fetch_user(1);
    $api->fetch_user(2);
    is($call_count, 2, 'fetch_user called twice');

    # Mock is automatically restored when $mock goes out of scope
};

# Bad: Monkey-patching without restoration
# *MyApp::API::fetch_user = sub { ... };  # NEVER — leaks across tests
```

For lightweight mock objects, use `Test::MockObject` to create injectable test doubles with `->mock()` and verify calls with `->called_ok()`.

## Coverage with Devel::Cover

### Running Coverage

```bash
# Basic coverage report
cover -test

# Or step by step
perl -MDevel::Cover -Ilib t/unit/user.t
cover

# HTML report
cover -report html
open cover_db/coverage.html

# Specific thresholds
cover -test -report text | grep 'Total'

# CI-friendly: fail under threshold
cover -test && cover -report text -select '^lib/' \
  | perl -ne 'if (/Total.*?(\d+\.\d+)/) { exit 1 if $1 < 80 }'
```

### Integration Testing

Use in-memory SQLite for database tests, mock HTTP::Tiny for API tests.

```perl
use v5.36;
use Test2::V0;
use DBI;

subtest 'database integration' => sub {
    my $dbh = DBI->connect('dbi:SQLite:dbname=:memory:', '', '', {
        RaiseError => 1,
    });
    $dbh->do('CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)');

    $dbh->prepare('INSERT INTO users (name) VALUES (?)')->execute('Alice');
    my $row = $dbh->selectrow_hashref('SELECT * FROM users WHERE name = ?', undef, 'Alice');
    is($row->{name}, 'Alice', 'inserted and retrieved user');
};

done_testing;
```

## Best Practices

### DO

- **Follow TDD**: Write tests before implementation (red-green-refactor)
- **Use Test2::V0**: Modern assertions, better diagnostics
- **Use subtests**: Group related assertions, isolate state
- **Mock external dependencies**: Network, database, file system
- **Use `prove -l`**: Always include lib/ in `@INC`
- **Name tests clearly**: `'user login with invalid password fails'`
- **Test edge cases**: Empty strings, undef, zero, boundary values
- **Aim for 80%+ coverage**: Focus on business logic paths
- **Keep tests fast**: Mock I/O, use in-memory databases

### DON'T

- **Don't test implementation**: Test behavior and output, not internals
- **Don't share state between subtests**: Each subtest should be independent
- **Don't skip `done_testing`**: Ensures all planned tests ran
- **Don't over-mock**: Mock boundaries only, not the code under test
- **Don't use `Test::More` for new projects**: Prefer Test2::V0
- **Don't ignore test failures**: All tests must pass before merge
- **Don't test CPAN modules**: Trust libraries to work correctly
- **Don't write brittle tests**: Avoid over-specific string matching

## Quick Reference

| Task | Command / Pattern |
|---|---|
| Run all tests | `prove -lr t/` |
| Run one test verbose | `prove -lv t/unit/user.t` |
| Parallel test run | `prove -lr -j8 t/` |
| Coverage report | `cover -test && cover -report html` |
| Test equality | `is($got, $expected, 'label')` |
| Deep comparison | `is($got, hash { field k => 'v'; etc() }, 'label')` |
| Test exception | `like(dies { ... }, qr/msg/, 'label')` |
| Test no exception | `ok(lives { ... }, 'label')` |
| Mock a method | `Test::MockModule->new('Pkg')->mock(m => sub { ... })` |
| Skip tests | `SKIP: { skip 'reason', $count unless $cond; ... }` |
| TODO tests | `TODO: { local $TODO = 'reason'; ... }` |

## Common Pitfalls

### Forgetting `done_testing`

```perl
# Bad: Test file runs but doesn't verify all tests executed
use Test2::V0;
is(1, 1, 'works');
# Missing done_testing — silent bugs if test code is skipped

# Good: Always end with done_testing
use Test2::V0;
is(1, 1, 'works');
done_testing;
```

### Missing `-l` Flag

```bash
# Bad: Modules in lib/ not found
prove t/unit/user.t
# Can't locate MyApp/User.pm in @INC

# Good: Include lib/ in @INC
prove -l t/unit/user.t
```

### Over-Mocking

Mock the *dependency*, not the code under test. If your test only verifies that a mock returns what you told it to, it tests nothing.

### Test Pollution

Use `my` variables inside subtests — never `our` — to prevent state leaking between tests.

**Remember**: Tests are your safety net. Keep them fast, focused, and independent. Use Test2::V0 for new projects, prove for running, and Devel::Cover for accountability.


---

---
name: perl-security
description: Comprehensive Perl security covering taint mode, input validation, safe process execution, DBI parameterized queries, web security (XSS/SQLi/CSRF), and perlcritic security policies.
origin: ECC
---

# Perl Security Patterns

Comprehensive security guidelines for Perl applications covering input validation, injection prevention, and secure coding practices.

## When to Activate

- Handling user input in Perl applications
- Building Perl web applications (CGI, Mojolicious, Dancer2, Catalyst)
- Reviewing Perl code for security vulnerabilities
- Performing file operations with user-supplied paths
- Executing system commands from Perl
- Writing DBI database queries

## How It Works

Start with taint-aware input boundaries, then move outward: validate and untaint inputs, keep filesystem and process execution constrained, and use parameterized DBI queries everywhere. The examples below show the safe defaults this skill expects you to apply before shipping Perl code that touches user input, the shell, or the network.

## Taint Mode

Perl's taint mode (`-T`) tracks data from external sources and prevents it from being used in unsafe operations without explicit validation.

### Enabling Taint Mode

```perl
#!/usr/bin/perl -T
use v5.36;

# Tainted: anything from outside the program
my $input    = $ARGV[0];        # Tainted
my $env_path = $ENV{PATH};      # Tainted
my $form     = <STDIN>;         # Tainted
my $query    = $ENV{QUERY_STRING}; # Tainted

# Sanitize PATH early (required in taint mode)
$ENV{PATH} = '/usr/local/bin:/usr/bin:/bin';
delete @ENV{qw(IFS CDPATH ENV BASH_ENV)};
```

### Untainting Pattern

```perl
use v5.36;

# Good: Validate and untaint with a specific regex
sub untaint_username($input) {
    if ($input =~ /^([a-zA-Z0-9_]{3,30})$/) {
        return $1;  # $1 is untainted
    }
    die "Invalid username: must be 3-30 alphanumeric characters\n";
}

# Good: Validate and untaint a file path
sub untaint_filename($input) {
    if ($input =~ m{^([a-zA-Z0-9._-]+)$}) {
        return $1;
    }
    die "Invalid filename: contains unsafe characters\n";
}

# Bad: Overly permissive untainting (defeats the purpose)
sub bad_untaint($input) {
    $input =~ /^(.*)$/s;
    return $1;  # Accepts ANYTHING — pointless
}
```

## Input Validation

### Allowlist Over Blocklist

```perl
use v5.36;

# Good: Allowlist — define exactly what's permitted
sub validate_sort_field($field) {
    my %allowed = map { $_ => 1 } qw(name email created_at updated_at);
    die "Invalid sort field: $field\n" unless $allowed{$field};
    return $field;
}

# Good: Validate with specific patterns
sub validate_email($email) {
    if ($email =~ /^([a-zA-Z0-9._%+-]+\@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,})$/) {
        return $1;
    }
    die "Invalid email address\n";
}

sub validate_integer($input) {
    if ($input =~ /^(-?\d{1,10})$/) {
        return $1 + 0;  # Coerce to number
    }
    die "Invalid integer\n";
}

# Bad: Blocklist — always incomplete
sub bad_validate($input) {
    die "Invalid" if $input =~ /[<>"';&|]/;  # Misses encoded attacks
    return $input;
}
```

### Length Constraints

```perl
use v5.36;

sub validate_comment($text) {
    die "Comment is required\n"        unless length($text) > 0;
    die "Comment exceeds 10000 chars\n" if length($text) > 10_000;
    return $text;
}
```

## Safe Regular Expressions

### ReDoS Prevention

Catastrophic backtracking occurs with nested quantifiers on overlapping patterns.

```perl
use v5.36;

# Bad: Vulnerable to ReDoS (exponential backtracking)
my $bad_re = qr/^(a+)+$/;           # Nested quantifiers
my $bad_re2 = qr/^([a-zA-Z]+)*$/;   # Nested quantifiers on class
my $bad_re3 = qr/^(.*?,){10,}$/;    # Repeated greedy/lazy combo

# Good: Rewrite without nesting
my $good_re = qr/^a+$/;             # Single quantifier
my $good_re2 = qr/^[a-zA-Z]+$/;     # Single quantifier on class

# Good: Use possessive quantifiers or atomic groups to prevent backtracking
my $safe_re = qr/^[a-zA-Z]++$/;             # Possessive (5.10+)
my $safe_re2 = qr/^(?>a+)$/;                # Atomic group

# Good: Enforce timeout on untrusted patterns
use POSIX qw(alarm);
sub safe_match($string, $pattern, $timeout = 2) {
    my $matched;
    eval {
        local $SIG{ALRM} = sub { die "Regex timeout\n" };
        alarm($timeout);
        $matched = $string =~ $pattern;
        alarm(0);
    };
    alarm(0);
    die $@ if $@;
    return $matched;
}
```

## Safe File Operations

### Three-Argument Open

```perl
use v5.36;

# Good: Three-arg open, lexical filehandle, check return
sub read_file($path) {
    open my $fh, '<:encoding(UTF-8)', $path
        or die "Cannot open '$path': $!\n";
    local $/;
    my $content = <$fh>;
    close $fh;
    return $content;
}

# Bad: Two-arg open with user data (command injection)
sub bad_read($path) {
    open my $fh, $path;        # If $path = "|rm -rf /", runs command!
    open my $fh, "< $path";   # Shell metacharacter injection
}
```

### TOCTOU Prevention and Path Traversal

```perl
use v5.36;
use Fcntl qw(:DEFAULT :flock);
use File::Spec;
use Cwd qw(realpath);

# Atomic file creation
sub create_file_safe($path) {
    sysopen(my $fh, $path, O_WRONLY | O_CREAT | O_EXCL, 0600)
        or die "Cannot create '$path': $!\n";
    return $fh;
}

# Validate path stays within allowed directory
sub safe_path($base_dir, $user_path) {
    my $real = realpath(File::Spec->catfile($base_dir, $user_path))
        // die "Path does not exist\n";
    my $base_real = realpath($base_dir)
        // die "Base dir does not exist\n";
    die "Path traversal blocked\n" unless $real =~ /^\Q$base_real\E(?:\/|\z)/;
    return $real;
}
```

Use `File::Temp` for temporary files (`tempfile(UNLINK => 1)`) and `flock(LOCK_EX)` to prevent race conditions.

## Safe Process Execution

### List-Form system and exec

```perl
use v5.36;

# Good: List form — no shell interpolation
sub run_command(@cmd) {
    system(@cmd) == 0
        or die "Command failed: @cmd\n";
}

run_command('grep', '-r', $user_pattern, '/var/log/app/');

# Good: Capture output safely with IPC::Run3
use IPC::Run3;
sub capture_output(@cmd) {
    my ($stdout, $stderr);
    run3(\@cmd, \undef, \$stdout, \$stderr);
    if ($?) {
        die "Command failed (exit $?): $stderr\n";
    }
    return $stdout;
}

# Bad: String form — shell injection!
sub bad_search($pattern) {
    system("grep -r '$pattern' /var/log/app/");  # If $pattern = "'; rm -rf / #"
}

# Bad: Backticks with interpolation
my $output = `ls $user_dir`;   # Shell injection risk
```

Also use `Capture::Tiny` for capturing stdout/stderr from external commands safely.

## SQL Injection Prevention

### DBI Placeholders

```perl
use v5.36;
use DBI;

my $dbh = DBI->connect($dsn, $user, $pass, {
    RaiseError => 1,
    PrintError => 0,
    AutoCommit => 1,
});

# Good: Parameterized queries — always use placeholders
sub find_user($dbh, $email) {
    my $sth = $dbh->prepare('SELECT * FROM users WHERE email = ?');
    $sth->execute($email);
    return $sth->fetchrow_hashref;
}

sub search_users($dbh, $name, $status) {
    my $sth = $dbh->prepare(
        'SELECT * FROM users WHERE name LIKE ? AND status = ? ORDER BY name'
    );
    $sth->execute("%$name%", $status);
    return $sth->fetchall_arrayref({});
}

# Bad: String interpolation in SQL (SQLi vulnerability!)
sub bad_find($dbh, $email) {
    my $sth = $dbh->prepare("SELECT * FROM users WHERE email = '$email'");
    # If $email = "' OR 1=1 --", returns all users
    $sth->execute;
    return $sth->fetchrow_hashref;
}
```

### Dynamic Column Allowlists

```perl
use v5.36;

# Good: Validate column names against an allowlist
sub order_by($dbh, $column, $direction) {
    my %allowed_cols = map { $_ => 1 } qw(name email created_at);
    my %allowed_dirs = map { $_ => 1 } qw(ASC DESC);

    die "Invalid column: $column\n"    unless $allowed_cols{$column};
    die "Invalid direction: $direction\n" unless $allowed_dirs{uc $direction};

    my $sth = $dbh->prepare("SELECT * FROM users ORDER BY $column $direction");
    $sth->execute;
    return $sth->fetchall_arrayref({});
}

# Bad: Directly interpolating user-chosen column
sub bad_order($dbh, $column) {
    $dbh->prepare("SELECT * FROM users ORDER BY $column");  # SQLi!
}
```

### DBIx::Class (ORM Safety)

```perl
use v5.36;

# DBIx::Class generates safe parameterized queries
my @users = $schema->resultset('User')->search({
    status => 'active',
    email  => { -like => '%@example.com' },
}, {
    order_by => { -asc => 'name' },
    rows     => 50,
});
```

## Web Security

### XSS Prevention

```perl
use v5.36;
use HTML::Entities qw(encode_entities);
use URI::Escape qw(uri_escape_utf8);

# Good: Encode output for HTML context
sub safe_html($user_input) {
    return encode_entities($user_input);
}

# Good: Encode for URL context
sub safe_url_param($value) {
    return uri_escape_utf8($value);
}

# Good: Encode for JSON context
use JSON::MaybeXS qw(encode_json);
sub safe_json($data) {
    return encode_json($data);  # Handles escaping
}

# Template auto-escaping (Mojolicious)
# <%= $user_input %>   — auto-escaped (safe)
# <%== $raw_html %>    — raw output (dangerous, use only for trusted content)

# Template auto-escaping (Template Toolkit)
# [% user_input | html %]  — explicit HTML encoding

# Bad: Raw output in HTML
sub bad_html($input) {
    print "<div>$input</div>";  # XSS if $input contains <script>
}
```

### CSRF Protection

```perl
use v5.36;
use Crypt::URandom qw(urandom);
use MIME::Base64 qw(encode_base64url);

sub generate_csrf_token() {
    return encode_base64url(urandom(32));
}
```

Use constant-time comparison when verifying tokens. Most web frameworks (Mojolicious, Dancer2, Catalyst) provide built-in CSRF protection — prefer those over hand-rolled solutions.

### Session and Header Security

```perl
use v5.36;

# Mojolicious session + headers
$app->secrets(['long-random-secret-rotated-regularly']);
$app->sessions->secure(1);          # HTTPS only
$app->sessions->samesite('Lax');

$app->hook(after_dispatch => sub ($c) {
    $c->res->headers->header('X-Content-Type-Options' => 'nosniff');
    $c->res->headers->header('X-Frame-Options'        => 'DENY');
    $c->res->headers->header('Content-Security-Policy' => "default-src 'self'");
    $c->res->headers->header('Strict-Transport-Security' => 'max-age=31536000; includeSubDomains');
});
```

## Output Encoding

Always encode output for its context: `HTML::Entities::encode_entities()` for HTML, `URI::Escape::uri_escape_utf8()` for URLs, `JSON::MaybeXS::encode_json()` for JSON.

## CPAN Module Security

- **Pin versions** in cpanfile: `requires 'DBI', '== 1.643';`
- **Prefer maintained modules**: Check MetaCPAN for recent releases
- **Minimize dependencies**: Each dependency is an attack surface

## Security Tooling

### perlcritic Security Policies

```ini
# .perlcriticrc — security-focused configuration
severity = 3
theme = security + core

# Require three-arg open
[InputOutput::RequireThreeArgOpen]
severity = 5

# Require checked system calls
[InputOutput::RequireCheckedSyscalls]
functions = :builtins
severity = 4

# Prohibit string eval
[BuiltinFunctions::ProhibitStringyEval]
severity = 5

# Prohibit backtick operators
[InputOutput::ProhibitBacktickOperators]
severity = 4

# Require taint checking in CGI
[Modules::RequireTaintChecking]
severity = 5

# Prohibit two-arg open
[InputOutput::ProhibitTwoArgOpen]
severity = 5

# Prohibit bare-word filehandles
[InputOutput::ProhibitBarewordFileHandles]
severity = 5
```

### Running perlcritic

```bash
# Check a file
perlcritic --severity 3 --theme security lib/MyApp/Handler.pm

# Check entire project
perlcritic --severity 3 --theme security lib/

# CI integration
perlcritic --severity 4 --theme security --quiet lib/ || exit 1
```

## Quick Security Checklist

| Check | What to Verify |
|---|---|
| Taint mode | `-T` flag on CGI/web scripts |
| Input validation | Allowlist patterns, length limits |
| File operations | Three-arg open, path traversal checks |
| Process execution | List-form system, no shell interpolation |
| SQL queries | DBI placeholders, never interpolate |
| HTML output | `encode_entities()`, template auto-escape |
| CSRF tokens | Generated, verified on state-changing requests |
| Session config | Secure, HttpOnly, SameSite cookies |
| HTTP headers | CSP, X-Frame-Options, HSTS |
| Dependencies | Pinned versions, audited modules |
| Regex safety | No nested quantifiers, anchored patterns |
| Error messages | No stack traces or paths leaked to users |

## Anti-Patterns

```perl
# 1. Two-arg open with user data (command injection)
open my $fh, $user_input;               # CRITICAL vulnerability

# 2. String-form system (shell injection)
system("convert $user_file output.png"); # CRITICAL vulnerability

# 3. SQL string interpolation
$dbh->do("DELETE FROM users WHERE id = $id");  # SQLi

# 4. eval with user input (code injection)
eval $user_code;                         # Remote code execution

# 5. Trusting $ENV without sanitizing
my $path = $ENV{UPLOAD_DIR};             # Could be manipulated
system("ls $path");                      # Double vulnerability

# 6. Disabling taint without validation
($input) = $input =~ /(.*)/s;           # Lazy untaint — defeats purpose

# 7. Raw user data in HTML
print "<div>Welcome, $username!</div>";  # XSS

# 8. Unvalidated redirects
print $cgi->redirect($user_url);         # Open redirect
```

**Remember**: Perl's flexibility is powerful but requires discipline. Use taint mode for web-facing code, validate all input with allowlists, use DBI placeholders for every query, and encode all output for its context. Defense in depth — never rely on a single layer.
