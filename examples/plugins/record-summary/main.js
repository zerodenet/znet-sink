const summary = JSON.parse(hostCall(JSON.stringify({
  capability: "records.summary.read", scope: "selection:demo"
})));
({records: summary.records, total_bytes: summary.upload_bytes + summary.download_bytes});
