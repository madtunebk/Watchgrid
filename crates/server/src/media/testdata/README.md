`hevc.mp4` is a generated five-frame blue 320×180 test clip, with no audio
or B-frames. It checks HEVC sample/config preservation through live fMP4
and the recording writer. Regenerate with:

```sh
ffmpeg -y -f lavfi -i color=c=blue:s=320x180:r=5 -frames:v 5 -c:v libx265 -preset ultrafast -x265-params pools=1:frame-threads=1:bframes=0:repeat-headers=0:log-level=error -tag:v hvc1 hevc.mp4
```
