# erasure_code
erasure_code tool and toy repo just for learning

# build
```bash
cargo build --release
```

# usage
**command**
```bash
target/release/erasure_code --help
A demo cli tool for erasure-code learning

Usage: erasure_code <COMMAND>

Commands:
  create   Create erasure code block
  rebuild  Rebuild try to rebuild the source data from remain parts
  help     Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

**create**
```bash
target/release/erasure_code create   
Create erasure code block

Usage: erasure_code create [OPTIONS] -i <INPUT-FILE> -d <DATA_DIR>

Options:
  -i <INPUT-FILE>      
  -d <DATA_DIR>        
  -p <PATTERN>         [default: 3+2]
  -h, --help           Print help
```

**rebuild**
```bash
target/release/erasure_code rebuild                                                            2 ↵
Rebuild try to rebuild the source data from remain parts

Usage: erasure_code rebuild -d <DATA_DIR> -o <OUTPUT_FILE_NAME>

Options:
  -d <DATA_DIR>              
  -o <OUTPUT_FILE_NAME>      
  -h, --help                 Print help
```
