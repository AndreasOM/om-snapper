# om-snapper

A very basic commandline tool to download AWS EC2/EBS snapshots.

Works for me. Might not work for you.

## Usage Example

```
om-snapper get snap-0123456789abcdefx --continue
```

## Installation

### From source
`cargo build --release`

### From github release
Download from [github](https://github.com/AndreasOM/om-snapper/releases) and unpack binaries to a location of your choice.

### cargo
#### binstall
`cargo binstall om-snapper@0.5.0-alpha` (*Note:* Once a non alpha version is released you can omit the @...)

This is work in progress.

### homebrew

:TODO:

## Features
- [x] Allows to continue after cancel/abort
- [ ] File verification
- [ ] Multithreaded download
- [ ] Bandwidth limiting

## Notes

Progress bar might skip, as empty (all zero blocks) are not transfered

## Future

Pull requests are very welcome.
Feature requests and bug reports too.


I use this on a regular basis, so I intend to keep it "good enough for me".
