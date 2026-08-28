# Developer Workflow

## 1. Start from clean main branch

```bash
git checkout main
git pull
```

Ensure no local changes

```bash
git status -s
```

## 2. Generate new branch

```bash
./tools/new-version.sh
```

## 3. Commit changes

```bash
git add <files>
git commit -m "feat: <description>"
```

## 4. Update Changelog

```bash
./tools/changelog-update.sh
```

**Optionally**: Update changelog.json Summary and Notes

Generate CHANGELOG.md

```bash
./tools/genmd-changelog.sh
```

## 5. Merge into main

```bash
./tools/main-merge.sh
```

## 6. Push to GitHub

```bash
git push origin main
```

## 7. Push Tags to GitHub

```bash
git push origin --tags
```


