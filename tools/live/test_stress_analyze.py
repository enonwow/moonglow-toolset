"""Keep stress loss accounting honest across delayed pages and reused worksets."""
import contextlib
import gzip
import io
import json
from pathlib import Path
import tempfile
import unittest
from stress_analyze import analyze


class Delivery(unittest.TestCase):
    def test_late_drain_current_boot_and_compact_log_receipt(self):
        with tempfile.TemporaryDirectory(prefix="mglive-stress-analysis-") as temporary:
            root=Path(temporary)
            native={"seconds":300,"generated":{"speech":2},"resources":[{"seconds":0,"cpu_ticks":0},{"seconds":300,"cpu_ticks":100}]}
            clients={"resources":[{"seconds":0,"cpu_s":0,"private":10},{"seconds":300,"cpu_s":1,"private":20}]}
            (root/"native.json").write_text(json.dumps(native))
            (root/"clients.json").write_text(json.dumps(clients))
            packets=[]
            for stamp,report,seq,text in [(1000,1,1,"ST:1"),(1100,2,2,""),(1200,3,4,""),(302000,4,3,"ST:2"),(302100,5,5,"")]:
                data={"boot":"boot-1","seq":report,"areas":[],"events":[{"seq":seq,"kind":"chat" if text else "damage","text":text}]}
                if report>1:data["base_seq"]=report-1
                packets.append({"at_ms":stamp,"channel":"state","data":data})
            with gzip.open(root/"replay.jsonl.gz","wt",encoding="utf-8") as stream:
                for packet in packets:stream.write(json.dumps(packet)+"\n")
            current=root/"data/live/logs/boot1";current.mkdir(parents=True)
            path=current/"chat.jsonl"
            path.write_text("".join(json.dumps({"data":{"game_seq":seq}})+"\n" for seq in [1,2,2,4]))
            old=root/"data/live/logs/oldboot";old.mkdir()
            (old/"chat.jsonl").write_text(json.dumps({"data":{"game_seq":3}})+"\n")
            (root/"delivery-barrier.json").write_text(json.dumps({"boot":"boot-1","game_seq":6,"committed_game_seq":6}))
            with contextlib.redirect_stdout(io.StringIO()):analyze(root)
            window=json.loads((root/"first-300.json").read_text())
            self.assertEqual(window["events_arrived_within_window"],3)
            self.assertEqual(window["missing_after_drain"],0)
            delivery=json.loads((root/"analysis.json").read_text())["store_log_delivery"]
            self.assertEqual(delivery["duplicate_rows"],1)
            self.assertEqual(delivery["missing_within_logged_sequence_span"],1)
            self.assertEqual(delivery["unflushed_capture_tail"],1)
            durable=json.loads((root/"analysis.json").read_text())["durable_delivery"]
            self.assertEqual({k:durable[k] for k in ["produced_through_barrier","logged","lost","duplicates","observer_missing"]},
                             {"produced_through_barrier":6,"logged":3,"lost":3,"duplicates":1,"observer_missing":1})
            path.unlink()  # Our disposable log; the compact receipt must suffice.
            with contextlib.redirect_stdout(io.StringIO()):analyze(root)
            self.assertEqual(json.loads((root/"analysis.json").read_text())["store_log_delivery"],delivery)
            self.assertEqual(json.loads((root/"analysis.json").read_text())["durable_delivery"],durable)


if __name__=="__main__":unittest.main()
