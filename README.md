# om-snapper

Very basic commandline tool to download AWS EC2/EBS snapshots.

Works for me. Might not work for you.

## Usage Example

```
om-snapper get snap-0123456789abcdefx --continue
```

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
