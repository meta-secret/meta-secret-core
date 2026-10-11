# Test 15: CLI rejoin during recovery

Each block performs one membership-change scenario, then validates every surviving member:

1. A member starts one recovery request.
2. A different member approves removal of the selected old device. The pending recovery must become `Declined`.
3. The CLI joins as the replacement third member and must observe its fresh `Split` claim.
4. The remaining three members each initiate one recovery request. Another remaining member approves it, and the sender verifies the recovered secret.

The post-join sender and approver rotation is explicit in each scenario file:

| Block | Remaining members | Sender → approver, in order |
| --- | --- | --- |
| 1 | Web, Android, CLI | Web → Android; Android → CLI; CLI → Web |
| 2 | Web, iOS, CLI | Web → iOS; iOS → CLI; CLI → Web |
| 3 | Web, Android, CLI | Web → Android; Android → CLI; CLI → Web |

The removed iOS or Android device is not part of the post-join validation. The initial pending recovery is expected to be declined. Each of the three surviving members then initiates one recovery request; a different member approves it and the sender verifies the secret. The remaining receiver's pending state must then be cleared by the persisted completion event before the next recovery check.

For the CLI-sender → Web-approver check, the other native receiver may observe the request only after Web has already approved it. In that case the UI test reports `ACTION_SKIPPED_AFTER_TERMINAL` instead of waiting for the dismissal action. The orchestrator accepts that signal only for the exact cycle and step, and then verifies the exact CLI sender claim has `clientStatus=Done`; a generic xcodebuild termination is never treated as a pass.
